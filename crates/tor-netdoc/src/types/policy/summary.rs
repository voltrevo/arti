//! Precise IP and port policy summarisation algorithm
//!
//! We don't use macrology for v4/v6, instead writing things twice.
//! We don't fear copypasta errors because the type system almost
//! always prevents mixing v4 and v6 information.
//!
//! We're using [`iprange::IpRange`] for our IP address sets.
//! That *is* a trie, but it's a pretty unoptimised one:
//! every node is fully boxed and there is no layer elision.
//! But it *does* have a nice API.
//!
//! See [`Summariser`] for the algorithm.

use std::net::{Ipv4Addr, Ipv6Addr};

use derive_deftly::{Deftly, define_derive_deftly};
use ipnet::{IpNet, Ipv4Net, Ipv6Net};
use iprange::IpRange;
use rangemap::RangeInclusiveMap;
use void::{ResultVoidExt as _, Void};

use crate::rangemap_mutate_range;

use super::*;

//---------- support materials ----------

/// Ports are 16-bit.  Alias for clarity.
type Port = u16;

/// Range for all real ports (not zero)
const ALL_PORTS: RangeInclusive<Port> = 1..=u16::MAX;

/// `eprintln` but in tests only, and prefix with `"TPRINT "`
///
/// Called in the non-test code in various places, but elided other than actually in tests.
macro_rules! tprintln { { $($a:tt)* } => { { {
    #[cfg(test)]
    eprintln!("TPRINT {}", format_args!($($a)*));
} } } }

//---------- IpNet trait for dealing with IP version generically ----------

/// `Ipv4Net` or `Ipv6Net` - IP-version specific handling
trait Net: iprange::IpNet {
    /// How many bits?
    const ADDR_BITS: u8;

    /// Return a /0 netblock.
    fn all() -> Self;

    /// How many hosts in this netblock?
    ///
    /// If the answer is 2^128, gives 2^128-1 instead.
    fn host_count_saturating(&self) -> u128 {
        let shift = Self::ADDR_BITS - self.prefix_len();
        1_u128.checked_shl(shift.into()).unwrap_or(u128::MAX)
    }
}

impl Net for Ipv4Net {
    const ADDR_BITS: u8 = 32;

    fn all() -> Self {
        Ipv4Net::new(Ipv4Addr::UNSPECIFIED, 0).expect("should be OK")
    }
}

impl Net for Ipv6Net {
    const ADDR_BITS: u8 = 128;

    fn all() -> Self {
        Ipv6Net::new(Ipv6Addr::UNSPECIFIED, 0).expect("should be OK")
    }
}

//==================== principal algorithm ====================

//---------- working data structure ----------

/// State for summarisation algorithm
///
/// We walk the rules in *reverse order*.
/// The rules are semantically first-match, but we want to walk *all* the rules,
/// doing all the port updates in parallel, and updating the accept/reject state
/// as we go - i.e. last match wins.
#[derive(Debug)]
struct Summariser {
    /// Set of IP addresses we are rejecting for each port
    ///
    /// Invariants: port 0 is not in the map.
    /// Every other port has an entry in the map, even if it's just two empty IpRanges.
    reject: RangeInclusiveMap<Port, Rejects>,
}

/// Which V4 and V6 addresses we are rejecting for a particular port
#[derive(Debug, Default, PartialEq, Clone)]
struct Rejects {
    /// Rejections for V4
    v4: IpRange<Ipv4Net>,
    /// Rejections for V6
    v6: IpRange<Ipv6Net>,
}

//---------- data accumulation into Summariser ----------

impl Summariser {
    /// Start the summarisation algorithm
    fn start() -> Self {
        let mut reject = RangeInclusiveMap::new();
        reject.insert(ALL_PORTS, Rejects::default());
        Summariser { reject }
    }

    /// Apply `rule_kind` for `pat` (for all IP versions) to `self`
    ///
    /// Overwrites old information - so last update wins.
    ///
    /// Calls `Reject::apply_rule` once for every relevant combination of:
    ///
    ///  * port range (via [`rangemap_mutate_range`])
    ///  * address family (open-coded, two similar calls)
    fn apply_rule(&mut self, rule_kind: RuleKind, pat: &AddrPortPattern) {
        let (v4, v6) = match pat.addrs {
            IpPattern::All => (Some(Net::all()), Some(Net::all())),
            IpPattern::Net(IpNet::V4(n)) => (Some(n), None),
            IpPattern::Net(IpNet::V6(n)) => (None, Some(n)),
        };

        let ports = pat.ports.to_range();

        tprintln!("apply_rule {ports:?} {rule_kind:?} {pat:?}");
        rangemap_mutate_range(
            &mut self.reject,
            &ports,
            |rejects: &mut Option<Rejects>, ports| {
                let Some(rejects) = rejects else {
                    debug_assert_eq!(*ports, 0..=0);
                    return Ok(());
                };
                Rejects::apply_rule(&mut rejects.v4, rule_kind, v4, ports);
                Rejects::apply_rule(&mut rejects.v6, rule_kind, v6, ports);
                Ok::<_, Void>(())
            },
        )
        .void_unwrap();
    }
}

impl Rejects {
    /// Apply `rule_kind` for `addrs` to `reject` for IP version `N`
    fn apply_rule<N: Net>(
        reject: &mut IpRange<N>,
        rule_kind: RuleKind,
        addrs: Option<N>,
        #[cfg_attr(not(test), allow(unused))] // for debugging prints in tests, only
        ports: &RangeInclusive<u16>,
    ) {
        let Some(addrs) = addrs else {
            return;
        };
        match rule_kind {
            RuleKind::Accept => reject.remove(addrs),
            RuleKind::Reject => reject.add(addrs),
        };
        tprintln!("apply_rule  {ports:?} {rule_kind:?} {addrs:?} now reject={reject:?}");
    }
}

//---------- readout core ----------

impl Summariser {
    /// Calculate the summary policy for IP version `N`
    ///
    /// `select_rejects` should pick the corresponding field out of `Rejects`
    fn policy_for_one_ip_version<N: Net>(
        &self,
        select_rejects: impl Fn(&Rejects) -> &IpRange<N>,
        max_reject_count: u128,
    ) -> PortPolicy {
        let mut allowed = PortRanges::new();
        for (ports, reject_ranges) in self.reject.iter() {
            tprintln!(
                "ports {:20} rej.count,max={max_reject_count:x}",
                format!("{ports:?}"),
            );
            let outcome = 'outcome: {
                let mut reject_count = 0_u128;
                for net in select_rejects(reject_ranges) {
                    reject_count = reject_count.saturating_add(net.host_count_saturating());
                    tprintln!(
                        "ports {:20} rej.count,now={reject_count:x} including {net:?}",
                        format!("{ports:?}"),
                    );
                    if reject_count > max_reject_count {
                        break 'outcome RuleKind::Reject;
                    }
                }
                debug_assert!(reject_count <= max_reject_count);
                break 'outcome RuleKind::Accept;
            };
            tprintln!("ports {:22} {outcome:?}", format!("{ports:?}"));
            match outcome {
                RuleKind::Accept => {
                    let ports =
                        PortRange::from_range(ports.clone()).expect("bad range in rangemap");
                    allowed
                        .push_ordered(ports)
                        .expect("disordered output from rangemap");
                }
                RuleKind::Reject => {}
            }
        }
        PortPolicy::from_allowed_port_ranges(allowed)
    }
}

define_derive_deftly! {
    /// Define [`PortPolicies::from_summariser`].
    //
    // The `v4` and `v6` fields have the same type.
    // Using a macro makes the otherwise-easy copy-pasta bugs impossible.
    PortPolicies beta_deftly:

    $impl {
        /// Actually calculate the port policy summaries for both IP versions
        fn from_summariser(
            summariser: Summariser,
            thresh: &PortSummaryThresholds,
        ) -> PortPolicies {
            PortPolicies { $(
                $fname: summariser.policy_for_one_ip_version(|r| &r.$fname, thresh.$fname),
            ) }
        }
    }
}

//====================  primary entrypoint, and output type ====================

/// A pair of port policy summaries, one for IPv4 and one for IPv6
///
/// Returned by [`AddrPolicy::summarise_precise`].
#[derive(Debug, Clone, Eq, PartialEq, Hash, Deftly)]
#[derive_deftly(PortPolicies)]
#[allow(clippy::exhaustive_structs)] // New IP version would be a breaking change
pub struct PortPolicies {
    /// IPv4
    pub v4: PortPolicy,

    /// IPv6
    pub v6: PortPolicy,
}

impl AddrPolicy {
    /// Calculate port policy summaries using a precise but unhardened algorithm
    ///
    /// Returns two Exit Policy Summaries, one for for each of IPv4 and IPv6.
    /// <https://spec.torproject.org/dir-spec/computing-consensus.html#exit-summary>
    ///
    /// **Not generally suitable for use on untrusted input because
    /// there is no effort to limit the computational complexity.**
    ///
    /// Useful for a router, when calculating
    /// [`ipv6-policy`
    /// ](https://spec.torproject.org/dir-spec/server-descriptor-format.html#item:ipv6-policy)
    /// in its router descriptor, from its own (locally configured) accept/reject policy.
    ///
    /// The result is calculated according to
    /// [this rule](https://spec.torproject.org/dir-spec/computing-consensus.html#exit-summary:semantics):
    ///
    /// > A port should be summarised as accepted iff the full exit policy
    /// > permits “most” “public” addresses on that port.
    ///
    /// `summarise_precise` implements the rule precisely as specified there;
    /// not the hardened approximate algorithm used by dirauths for IPv4 summaries.
    ///
    /// `private_ranges` is the ranges considered not "public".
    /// Rejections of addresses in these ranges are disregarded when considering
    /// whether a port is open.
    ///
    /// This algorithm does not handle "IPv4-mapped Addresses"
    /// (ie, IPv6-mapped IPv4 addresses) specially.
    /// They should normally be rejected, and be in `private_ranges`.
    ///
    /// `thresholds` should normally be `&PortSummaryThresholds::DEFAULT`.
    //
    // To generate an `ipv6-policy` line, it would be sufficient to only calculate a v6 summary.
    // So why provide v4 too?  Because it's useful for testing of the approximate summary
    // algorithm, and because we might want to move v4 policy summarisation to relays, too.
    //
    // Why return both policies, rather than providing separate entrypoints?
    // Mostly, because it's convenient in the implementation: computing them separately
    // would mean more of our principal code would be generic over `N`.
    // This is not supposed to be a hot path anyway.
    pub fn summarise_precise(
        &self,
        thresholds: &PortSummaryThresholds,
        private_ranges: impl IntoIterator<Item = IpNet>,
    ) -> PortPolicies {
        let mut s = Summariser::start();
        for (rule_kind, pat) in self.rules().rev() {
            s.apply_rule(rule_kind, &pat);
        }

        tprintln!("summariser intermediate: {s:#?}");

        // We handle private ranges by deleting them from rejected list, pretending they're open

        let all_ports =
            PortRange::from_range(ALL_PORTS).expect("all ports is fixedly correct range");

        for private in private_ranges {
            s.apply_rule(
                RuleKind::Accept,
                &AddrPortPattern {
                    addrs: IpPattern::Net(private),
                    ports: all_ports,
                },
            );
        }

        tprintln!("summariser final: {s:#?}");

        PortPolicies::from_summariser(s, thresholds)
    }
}

//---------- PortSummaryThresholds configuration type ----------

/// Thresholds for deciding whether a port counts as open, for a summary
///
/// Each value is the maximum number of individual addresses
/// that may be blocked before the port is considered closed.
///
/// The `Default` implementation, and [`PortSummaryThresholds::DEFAULT`],
/// provide the thresholds currently specified in torspec.
///
/// We provide this as a controllable parameter so that the summariser is
/// a pure function that doesn't embed these tuneables.
/// Then if the spec changes,  the directory authority consensus calculator
/// can provide the appropriate thresholds depending on the consensus method.
#[derive(Debug, Clone, Eq, PartialEq, Hash, Deftly)]
#[allow(clippy::exhaustive_structs)] // New IP version would be a breaking change
#[derive_deftly(PortSummaryThresholds)]
pub struct PortSummaryThresholds {
    /// IPv4
    ///
    /// Currently, the spec says
    ///
    /// > no more than 2^25 IPv4 addresses (two /8's worth, or one /7's worth)
    #[deftly(default_prefix_len = 7)]
    pub v4: u128,

    /// IPv6
    ///
    /// Currently, the spec says
    ///
    /// > no more than 2^112 IPv6 addresses (one /16's worth)
    #[deftly(default_prefix_len = 16)]
    pub v6: u128,
}

define_derive_deftly! {
    /// Define impls on `PortSummaryThresholds`
    ///
    /// This is a macro because there's no type-based safeguard against copy-paste bugs.
    PortSummaryThresholds beta_deftly, meta_quoted rigorous:

    ${define N $<Ip $fname Net>}

    $impl {
        /// PortSummaryThresholds from prefix lengths
        ///
        /// Returns a `PortSummaryThresholds` whose thresholds are
        /// "one /`v4`'s worth" for IPv4
        /// and
        /// "one /`v6`'s worth" for IPv6.
        pub const fn from_prefix_lengths( $(
            $<$fname _prefix_len>: u8,
        ) ) -> PortSummaryThresholds {
            PortSummaryThresholds { $(
                $fname: 1_u128 << $N::ADDR_BITS - $<$fname _prefix_len>,
            ) }
        }

        /// Default value, from the Tor Specifications
        pub const DEFAULT: PortSummaryThresholds = PortSummaryThresholds::from_prefix_lengths( $(
            ${fmeta(default_prefix_len) as expr},
        ) );
    }
}
use derive_deftly_template_PortSummaryThresholds;

impl Default for PortSummaryThresholds {
    fn default() -> Self {
        PortSummaryThresholds::DEFAULT
    }
}

#[cfg(test)]
mod test {
    // @@ begin test lint list maintained by maint/add_warning @@
    #![allow(clippy::bool_assert_comparison)]
    #![allow(clippy::clone_on_copy)]
    #![allow(clippy::dbg_macro)]
    #![allow(clippy::mixed_attributes_style)]
    #![allow(clippy::print_stderr)]
    #![allow(clippy::print_stdout)]
    #![allow(clippy::single_char_pattern)]
    #![allow(clippy::unwrap_used)]
    #![allow(clippy::unchecked_time_subtraction)]
    #![allow(clippy::useless_vec)]
    #![allow(clippy::needless_pass_by_value)]
    #![allow(clippy::string_slice)] // See arti#2571
    //! <!-- @@ end test lint list maintained by maint/add_warning @@ -->
    use super::*;
    use crate::parse_testcase_from_netdoc;
    use itertools::Itertools;

    #[derive(Deftly)]
    #[derive_deftly(NetdocParseableFields)]
    struct TestCase {
        /// Input policy, `accept` and `reject` lines
        #[deftly(netdoc(flatten))]
        full: AddrPolicy,

        /// Expected IPv4 summary
        p4: PortPolicy,

        /// Expected IPv6 summary
        p6: PortPolicy,
    }

    /// Run one test case
    ///
    /// This is the implementation of `chk`.
    ///
    /// It returns `Result`, just for the benefit of its self-test.
    fn chk_inner(input_doc: &str) -> anyhow::Result<()> {
        let case: TestCase = parse_testcase_from_netdoc(input_doc);

        let summary = case.full.summarise_precise(
            &PortSummaryThresholds::DEFAULT,
            [
                // hardly a complete list
                "0.0.0.0/8",
                "::/8",
                "::faff:0:0/96",
                "10.0.0.0/8",
                "172.16.0.0/12",
                "192.168.0.0/16",
                "fd00::/8",
            ]
            .into_iter()
            .map(|s| s.parse::<IpNet>().expect(s))
            .collect_vec(),
        );

        /// Like `assert_eq`  combined with `anyhow::ensure` - throws `Err(anyhow::Error)`
        macro_rules! ensure_eq { { $a:expr, $b:expr } => {
            anyhow::ensure!($a == $b, "{:?} != {:?}", $a, $b);
        } }

        ensure_eq!(summary.v4, case.p4);
        ensure_eq!(summary.v6, case.p6);

        Ok(())
    }

    /// Run one test case
    ///
    /// Test cases are strings in netdoc format, for `TestCase`,
    /// but without the intro item.
    ///
    /// Whitespace will be normalised and `#`-comments stripped.
    fn chk(input_doc: &str) {
        chk_inner(input_doc).expect("test failed");
    }

    #[test]
    fn basics() {
        chk(r"
                p4 accept 1-65535
                p6 accept 1-65535
        ");
        chk(r"
                accept *:*
                p4 accept 1-65535
                p6 accept 1-65535
        ");
        chk(r"
                reject *:*
                p4 reject 1-65535
                p6 reject 1-65535
        ");
        chk(r"
                reject 0.0.0.0/0:*
                p4 reject 1-65535
                p6 accept 1-65535
        ");
        chk(r"
                reject [::]/0:*
                p4 accept 1-65535
                p6 reject 1-65535
        ");
    }

    #[test]
    fn edge_cases() {
        // Reject nearly enough addresses to reject
        let reject_precisely_allowed_amount = r"
                accept *:100
                reject 1.0.0.0/8:400-419
                reject 2.0.0.0/8:410-429
                reject [2002::]/17:600-619
                reject [2003::]/17:610-629
                reject 0.0.0.0/0:1-399
                reject 0.0.0.0/0:430-65535
                reject [::]/0:1-599
                reject [::]/0:630-65535
        ";

        chk(&format!(
            r"  {reject_precisely_allowed_amount}

                # reject some private nets, proving they are disregarded
                reject 10.0.0.0/8:*
                reject [fd00::]/8:*

                p4 accept 100,400-429
                p6 accept 100,600-629 "
        ));

        // Reject one more address
        chk(&format!(
            r"  {reject_precisely_allowed_amount}

                reject 4.0.0.0/32:415-425
                reject [2001:a::1]/128:615-625

                p4 accept 100,400-414,420-429
                p6 accept 100,600-614,620-629 "
        ));
    }

    #[test]
    fn chk_detects_discrepancies() {
        // The output is supposed to be `p4 reject 25` ...
        let input_doc = r"
                reject *:25
                p4 reject 26
                p6 reject 26
        ";
        let e = chk_inner(input_doc).expect_err("was supposed to fail");
        let e = format!("{e:#}"); // # to make anyhow print sources

        assert!(
            e.contains("!= PortPolicy { allowed: PortRanges([PortRange(1-25)"),
            "error: {e}"
        );
    }
}
