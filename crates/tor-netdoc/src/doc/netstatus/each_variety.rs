//! network status documents - items for all varieties, that vary
//!
//! **This file is reincluded multiple times**,
//! by the macros in [`crate::doc::ns_variety_definition_macros`],
//! once for votes, and once for each consensus flavour.
//! It is *not* a module `crate::doc::netstatus::rs::each_variety`.
//!
//! Each time this file is included by one of the macros mentioned above,
//! the `ns_***` macros (such as `ns_const_name!`) may expand to different values.
//!
//! See [`crate::doc::ns_variety_definition_macros`].

use super::*;

ns_use_this_variety! {
    pub use [crate::doc::netstatus::rs]::?::{RouterStatus};
}

/// `network-status-version` intro item in a consensus
///
/// This is hard to parse because it's so irregular (even, ambiguous).
///
/// <https://spec.torproject.org/dir-spec/consensus-formats.html#item:network-status-version>
///
/// <https://spec.torproject.org/dir-spec/computing-consensus.html#flavor:microdesc>
///
/// <https://gitlab.torproject.org/tpo/core/torspec/-/work_items/359>
#[derive(Clone, Debug, Deftly, Default)]
#[derive_deftly(Constructor, ItemValueEncodable, ItemValueParseable)]
#[allow(clippy::exhaustive_structs)]
pub struct NetworkStatusVersionItem {
    /// The version number, always `3`
    pub version: NetworkStatusVersion,

    /// The `flavor` argument
    ///
    ///  * In a plain consensus, this is an optional `ns`.
    ///  * In an md consensus, this is always `microdesc`.
    ///  * In a vote, there is no variety, but to avoid ambiguity, we reject.
    pub variety: VarietyKeyword,

    #[doc(hidden)]
    #[deftly(netdoc(skip))]
    pub __non_exhaustive: (),
}

/// The preamble of a network status document, except for the intro and `vote-status` items.
///
/// <https://spec.torproject.org/dir-spec/consensus-formats.html#section:preable>
///
/// **Does not include `network-status-version` and `vote-status`**.
/// In the old parser this is not represented directly;
/// instead, in `Consensus.flavor`, there's just the `ConsensusFlavor`.
/// `parse2` doesn't (currently) support subdocuments which contain the parent's intro item
/// (ie, `#[deftly(netdoc(flatten))]` is not supported on the first field.)
//
// TODO DIRAUTH this is missing some fields that need to be included in votes,
// by dirauths, when voting.  They are not needed for calculating a consensus from votes.
// there are individual TODO comments about each such defect.
#[derive(Clone, Debug, Deftly)]
#[derive_deftly(Constructor, NetdocEncodableFields, NetdocParseableFields)]
#[allow(clippy::exhaustive_structs)]
pub struct Preamble {
    /// Consensus methods supported by this voter.
    #[deftly(constructor)]
    pub consensus_methods: ns_type!( NotPresent, NotPresent, ConsensusMethods ),

    /// What "method" was used to produce this consensus?  (A
    /// consensus method is a version number used by authorities to
    /// upgrade the consensus algorithm.)
    #[deftly(constructor)]
    // Not #[deftly(netdoc(single_arg))] because that would mean a consensuses
    // had an always-present singleton `consensus_method` item with no arguments.
    pub consensus_method: ns_type!( (u32,), (u32,), NotPresent ),

    /// Publication time (of a vote)
    #[deftly(constructor)]
    // Not #[deftly(netdoc(single_arg))] because that would mean a consensuses
    // had an always-present singleton `published` item with no arguments.
    pub published: ns_type!( NotPresent, NotPresent, (Iso8601TimeSp,) ),

    /// Over what time is this consensus valid?  (For votes, this is
    /// the time over which the voted-upon consensus should be valid.)
    #[deftly(constructor)]
    #[deftly(netdoc(flatten))]
    pub lifetime: Lifetime,

    /// How long in seconds should voters wait for votes and
    /// signatures (respectively) to propagate?
    pub voting_delay: Option<(u32, u32)>,

    /// List of recommended Tor client versions.
    #[deftly(constructor)]
    #[deftly(netdoc(single_arg))]
    pub client_versions: Vec<String>,

    /// List of recommended Tor relay versions.
    #[deftly(constructor)]
    #[deftly(netdoc(single_arg))]
    pub server_versions: Vec<String>,

    /// Router flags which could be determined
    #[deftly(constructor)]
    #[deftly(netdoc(with = "relay_flags::ParserEncoder::<relay_flags::NoImplicitRepr>"))]
    pub known_flags: DocRelayFlags,

    // TODO DIRAUTH torspec#404 missing field: flag-thresholds (in votes)

    /// Lists of recommended and required subprotocols.
    ///
    /// **`{recommended,required}-{client,relay}-protocols`**
    #[deftly(constructor)]
    #[deftly(netdoc(flatten))]
    pub proto_statuses: Arc<ProtoStatuses>,

    /// Declared parameters for tunable settings about how to the
    /// network should operator. Some of these adjust timeouts and
    /// whatnot; some features things on and off.
    #[deftly(constructor)]
    pub params: NetParams<i32>,

    /// Global shared-random values
    #[deftly(netdoc(flatten))]
    pub shared_rand: ns_type!( SharedRandStatuses, SharedRandStatuses, NotPresent ),

    // TODO DIRAUTH missing field: bandwidth-file-headers (in votes)
    // TODO DIRAUTH missing field: bandwidth-file-digest (in votes)

    #[doc(hidden)]
    #[deftly(netdoc(skip))]
    pub __non_exhaustive: (),
}

/// The footer of a network status document.
///
/// <https://spec.torproject.org/dir-spec/consensus-formats.html#section:footer>>
#[derive(Clone, Debug, Deftly)]
#[derive_deftly(Constructor, NetdocEncodable, NetdocParseable)]
#[allow(clippy::exhaustive_structs)]
pub struct Footer {
    /// Intro item
    ///
    /// <https://spec.torproject.org/dir-spec/consensus-formats.html#item:directory-footer>
    pub directory_footer: (),

    /// Fields that appear in consensuses (only)
    #[deftly(constructor, netdoc(flatten))]
    pub consensus: ns_type!(ConsensusFooterFields, ConsensusFooterFields, NotPresent),

    #[doc(hidden)]
    #[deftly(netdoc(skip))]
    pub __non_exhaustive: (),
}

/// Signatures on a network status document
#[derive(Deftly, Clone, Debug)]
#[derive_deftly(NetdocParseableSignatures)]
#[deftly(netdoc(signatures(hashes_accu = "DirectorySignaturesHashesAccu")))]
#[non_exhaustive]
pub struct NetworkStatusSignatures {
    /// `directory-signature`s
    pub directory_signature: ns_type!(Vec<Signature>, Vec<Signature>, Signature),
}
