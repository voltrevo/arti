//! Raw keystore entry identifiers used in plumbing CLI functionalities.

use std::path::PathBuf;

use tor_basic_utils::PathExt;
use tor_key_forge::KeystoreItemType;

use crate::ArtiPath;

/// The raw identifier of a key inside a [`Keystore`](crate::Keystore).
///
/// The exact type of the identifier depends on the backing storage of the keystore
/// (for example, an on-disk keystore will identify its entries by [`Path`](RawEntryId::Path)).
#[cfg_attr(
    any(feature = "onion-service-cli-extra", feature = "experimental-api"),
    visibility::make(pub)
)]
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, derive_more::Display)]
pub(crate) enum RawEntryId {
    /// An entry identified by path inside an on-disk keystore.
    // NOTE: this will only be used by on-disk keystores like
    // [`ArtiNativeKeystore`](crate::ArtiNativeKeystore)
    #[display("{}", _0.display_lossy())]
    Path(PathBuf),

    /// An entry of an in-memory ephemeral key storage
    /// [`ArtiEphemeralKeystore`](crate::ArtiEphemeralKeystore)
    ///
    // TODO: the concept of a "raw identifier" doesn't really make sense
    // in the context of the `ArtiEphemeralKeystore`,
    // which is why this "raw" identifier is of exactly the same type
    // (`(ArtiPath, KeystoreItemType)`) as its non-"raw" counterpart.
    // Ephemeral keystores are just in-memory key-value mappings;
    // unlike file system-based keystores, these don't have entries with "raw"
    // identifiers that need to be validated and parsed before they can be used.
    //
    // We might want to remove this variant entirely,
    // and make `RawEntryId` optional in e.g. `KeystoreEntry`.
    #[display("{} {:?}", _0.0, _0.1)]
    Ephemeral((ArtiPath, KeystoreItemType)),
    // TODO: when/if we add support for non on-disk keystores,
    // new variants will be added
}
