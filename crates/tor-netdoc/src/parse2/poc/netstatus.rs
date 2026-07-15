//! network status documents: shared between votes, consensuses and md consensuses

use super::*;

use crate::doc::{self, authcert};
use crate::types;
use authcert::AuthCert as DirAuthKeyCert;
pub use doc::netstatus::Signature as NdiDirectorySignature;
use doc::netstatus::{
    ConsensusAuthoritySection, DirectorySignaturesHashesAccu, VerifyGeneralTrustedAuthorities,
    VoteAuthoritySection, VoteStatusConsensus, VoteStatusVote,
};

mod ns_per_flavour_macros;
pub use ns_per_flavour_macros::*;

ns_per_flavour_macros::ns_export_flavoured_types! {
    NetworkStatus, NetworkStatusUnverified, Router,
}

/// `params` value
#[derive(Clone, Debug, Default, Deftly)]
#[derive_deftly(ItemValueParseable)]
#[non_exhaustive]
pub struct NdiParams {
    // Not implemented.
}

/// `r` sub-document
#[derive(Deftly, Clone, Debug)]
#[derive_deftly(ItemValueParseable)]
#[non_exhaustive]
pub struct NdiR {
    /// nickname
    pub nickname: types::Nickname,
    /// identity
    pub identity: String, // In non-demo, use a better type
}

/// Meat of the verification functions for network documents
///
/// Checks that at least `threshold` members of `trusted`
/// have signed this document (in `signatures`),
/// via some cert(s) in `certs`.
///
/// Does not check validity time.
fn verify_general_timeless(
    hashes: &DirectorySignaturesHashesAccu,
    signatures: &[NdiDirectorySignature],
    trusted: &[pk::rsa::RsaIdentity],
    certs: &[&DirAuthKeyCert],
) -> Result<(), VF> {
    let group = crate::doc::netstatus::SignatureGroup {
        hashes: *hashes,
        signatures: signatures.iter().cloned().collect_vec(),
    };

    group.verify_general(
        VerifyGeneralTrustedAuthorities::TrustThese { trusted },
        &certs.iter().copied().cloned().collect_vec(),
        |tv| tv.verify(),
    )
}
