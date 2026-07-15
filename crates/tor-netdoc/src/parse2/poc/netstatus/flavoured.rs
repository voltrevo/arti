//! network status documents - types that vary by flavour
//!
//! **This file is reincluded multiple times**,
//! once for each consensus flavour, and once for votes.
//!
//! Each time, with different behaviour for the macros `ns_***`.
//!
//! Thus, this file generates (for example) all three of:
//! `ns::NetworkStatus` aka `NetworkStatusNs`,
//! `NetworkStatusMd` and `NetworkStatusVote`.
//!
//! (We treat votes as a "flavour".)

use super::super::*;

/// Toplevel document string for error reporting
const TOPLEVEL_DOCTYPE_FOR_ERROR: &str =
    ns_expr!("NetworkStatusVote", "NetworkStatusNs", "NetworkStatusMd",);

/// The real router status entry type.
pub type Router = ns_type!(
    crate::doc::netstatus::VoteRouterStatus,
    crate::doc::netstatus::PlainRouterStatus,
    crate::doc::netstatus::MdRouterStatus,
);

/// The real footer type.
pub type NddDirectoryFooter = ns_type!(
    crate::doc::netstatus::VoteFooter,
    crate::doc::netstatus::PlainFooter,
    crate::doc::netstatus::MdFooter,
);

/// The real signatures section type.
pub type NetworkStatusSignatures = ns_type!(
    crate::doc::netstatus::vote::NetworkStatusSignatures,
    crate::doc::netstatus::plain::NetworkStatusSignatures,
    crate::doc::netstatus::md::NetworkStatusSignatures,
);

/// The real `network-status-version` item type.
pub type NetworkStatusVersionItem = ns_type!(
    crate::doc::netstatus::vote::NetworkStatusVersionItem,
    crate::doc::netstatus::plain::NetworkStatusVersionItem,
    crate::doc::netstatus::md::NetworkStatusVersionItem,
);

/// Network status document (vote, consensus, or microdescriptor consensus) - body
///
/// The preamble items are members of this struct.
/// The rest are handled as sub-documents.
#[derive(Deftly, Clone, Debug)]
#[derive_deftly(NetdocParseableUnverified)]
#[deftly(netdoc(doctype_for_error = TOPLEVEL_DOCTYPE_FOR_ERROR))]
#[non_exhaustive]
pub struct NetworkStatus {
    /// `network-status-version`
    pub network_status_version: NetworkStatusVersionItem,

    /// `vote-status`
    pub vote_status: NdiVoteStatus,

    /// `published`
    pub published: ns_type!((NdaSystemTimeDeprecatedSyntax,), Option<Void>,),

    /// `valid-after`
    pub valid_after: (NdaSystemTimeDeprecatedSyntax,),

    /// `valid-until`
    pub valid_until: (NdaSystemTimeDeprecatedSyntax,),

    /// `voting-delay`
    pub voting_delay: NdiVotingDelay,

    /// `params`
    #[deftly(netdoc(default))]
    pub params: NdiParams,

    /// Authority section
    #[deftly(netdoc(subdoc))]
    pub authority: NddAuthoritySection,

    /// `r` subdocuments
    #[deftly(netdoc(subdoc))]
    pub r: Vec<Router>,

    /// `directory-footer` section (which we handle as a sub-document)
    #[deftly(netdoc(subdoc))]
    pub directory_footer: Option<NddDirectoryFooter>,
}

/// `vote-status` value
///
/// In a non-demo we'd probably abolish this,
/// using `NdaStatus` directly in `NddNetworkStatus`
/// impl of `ItemValueParseable` for tuples.
#[derive(Deftly, Clone, Debug, Hash, Eq, PartialEq)]
#[derive_deftly(ItemValueParseable)]
#[non_exhaustive]
pub struct NdiVoteStatus {
    /// status
    pub status: ns_type!(VoteStatusVote, VoteStatusConsensus, VoteStatusConsensus),
}

/// `voting-delay` value
#[derive(Deftly, Clone, Debug, Hash, Eq, PartialEq)]
#[derive_deftly(ItemValueParseable)]
#[non_exhaustive]
pub struct NdiVotingDelay {
    /// VoteSeconds
    pub vote_seconds: u32,
    /// DistSeconds
    pub dist_seconds: u32,
}

/// `dir-source`
#[derive(Deftly, Clone, Debug)]
#[derive_deftly(ItemValueParseable)]
#[non_exhaustive]
pub struct NdiAuthorityDirSource {
    /// nickname
    pub nickname: types::Nickname,
    /// fingerprint
    pub h_p_auth_id_rsa: types::Fingerprint,
}

ns_choose! { (
    use VoteAuthoritySection as NddAuthoritySection;
)(
    use ConsensusAuthoritySection as NddAuthoritySection;
)}

ns_choose! { (
    impl NetworkStatusUnverified {
        /// Verify this vote's signatures using the embedded certificate
        ///
        /// # Security considerations
        ///
        /// The caller should use `NetworkStatus::h_kp_auth_id_rsa`
        /// to find out which voter's vote this is.
        pub fn verify_selfcert(
            self,
            now: SystemTime,
        ) -> Result<(NetworkStatus, SignaturesData<NetworkStatusUnverified>), VF> {
            let validity = *self.body.published.0 ..= *self.body.valid_until.0;
            check_validity_time(now, validity)?;

            let cert = self.body.parse_authcert()?.verify_selfcert(now)?;

            netstatus::verify_general_timeless(
                &self.sigs.hashes,
                slice::from_ref(&self.sigs.sigs.directory_signature),
                &[*cert.fingerprint],
                &[&cert],
            )?;

            Ok(self.unwrap_unverified())
        }
    }

    impl NetworkStatus {
        /// Parse the embedded authcert
        //
        // TODO DIRAUTH abolish/move
        fn parse_authcert(&self) -> Result<crate::doc::authcert::AuthCertUnverified, EP> {
            let cert_input = ParseInput::new(
                self.authority.cert.raw_unverified().as_str(),
                "<embedded auth cert>",
            );
            parse_netdoc(&cert_input).map_err(|e| e.problem)
        }

        /// Voter identity
        ///
        /// # Security considerations
        ///
        /// The returned identity has been confirmed to have properly certified
        /// this vote at this time.
        ///
        /// It is up to the caller to decide whether this identity is actually
        /// a voter, count up votes, etc.
        //
        // TODO DIRAUTH use EmbeddedCert::get
        pub fn h_kp_auth_id_rsa(&self) -> pk::rsa::RsaIdentity {
            *self.parse_authcert()
                // SECURITY: if the user calls this function, they have a bare
                // NetworkStatus, not a NetworkStatusUnverified, so parsing
                // and verification has already been done in verify_selfcert above.
                .expect("was verified already!")
                .inspect_unverified()
                .0
                .fingerprint
        }
    }
) (
    impl NetworkStatusUnverified {
        /// Verify this consensus document
        ///
        /// # Security considerations
        ///
        /// The timeliness verification is relaxed, and incorporates the `DistSeconds` skew.
        /// The caller **must not use** the returned consensus before its `valid_after`,
        /// and must handle `fresh_until`.
        ///
        /// `authorities` should be a list of the authorities
        /// that the caller trusts.
        ///
        /// `certs` is a list of dir auth key certificates to use to try to link
        /// the signed consensus to those authorities.
        /// Extra certificates in `certs`, that don't come from anyone in `authorities`,
        /// are ignored.
        pub fn verify(
            self,
            now: SystemTime,
            authorities: &[pk::rsa::RsaIdentity],
            certs: &[&DirAuthKeyCert],
        ) -> Result<(NetworkStatus, SignaturesData<NetworkStatusUnverified>), VF> {
            let validity_start = self.body.valid_after.0
                .checked_sub(Duration::from_secs(self.body.voting_delay.dist_seconds.into()))
                .ok_or(VF::Other)?;
            check_validity_time(now, validity_start..= *self.body.valid_until.0)?;

            netstatus::verify_general_timeless(
                &self.sigs.hashes,
                &self.sigs.sigs.directory_signature,
                authorities,
                certs,
            )?;

            Ok(self.unwrap_unverified())
        }
    }
)}
