//! network status documents: shared between votes, consensuses and md consensuses

use super::*;

use crate::doc::{self, authcert};
use crate::types;
use authcert::{AuthCert as DirAuthKeyCert, AuthCertKeyIds};
pub use doc::netstatus::Signature as NdiDirectorySignature;
use doc::netstatus::{
    ConsensusAuthoritySection, DirectorySignaturesHashesAccu, VoteAuthoritySection,
    VoteStatusConsensus, VoteStatusVote,
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
    threshold: usize,
) -> Result<(), VF> {
    let mut ok = HashSet::<pk::rsa::RsaIdentity>::new();

    for sig in signatures {
        let NdiDirectorySignature {
            digest_algo: hash_algo,
            key_ids:
                AuthCertKeyIds {
                    id_fingerprint: h_kp_auth_id_rsa,
                    sk_fingerprint: h_kp_auth_sign_rsa,
                },
            signature: rsa_signature,
        } = sig;

        if let Some(h) = hashes.hash_slice_for_verification(hash_algo) {
            let Some(authority) = ({
                trusted
                    .iter()
                    .find(|trusted| **trusted == *h_kp_auth_id_rsa)
            }) else {
                // unknown kp_auth_id_rsa, ignore it
                continue;
            };
            let Some(cert) = ({
                certs
                    .iter()
                    .find(|cert| cert.dir_signing_key.to_rsa_identity() == *h_kp_auth_sign_rsa)
            }) else {
                // no cert for this kp_auth_sign_rsa, ignore it
                continue;
            };

            let () = cert.dir_signing_key.verify(h, rsa_signature)?;

            ok.insert(*authority);
        }
    }

    if ok.len() < threshold {
        return Err(VF::InsufficientTrustedSigners);
    }

    Ok(())
}
