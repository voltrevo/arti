//! Implementation for the style of router status entries used in
//! microdesc consensus documents.
//
// Read this file in conjunction with `each_variety.rs`.
// See "module scope" ns_variety_definition_macros.rs.

use super::*;

// Import `each_variety.rs`, appropriately variegated
ns_do_variety_md! {}

// We bind some variety-agnostic names for the benefit of `each_variety.rs`,
// which reimports the contents of this module with `use super::*`.
pub(crate) use crate::doc::microdesc::{DOC_DIGEST_LEN, MdDigest as DocDigest};

/// The flavor
const FLAVOR: ConsensusFlavor = ConsensusFlavor::Microdesc;

impl RouterStatus {
    /// Return the expected microdescriptor digest for this routerstatus
    pub fn md_digest(&self) -> &DocDigest {
        self.doc_digest()
    }
}

/// Netdoc format helper module for referenced doc digest field in `m` (where it's an item)
///
/// See `doc_digest_parse2_real` in `rs/each_variety.rs`.
/// This is in `md.rs` because it's needed only for md consensuses.
/// Elsewhere, the value is in the `r` item, so is merely `ItemArgumentParseable`.
pub(crate) mod doc_digest_item_m {
    use super::*;
    use crate::parse2::ErrorProblem as EP;
    use crate::parse2::UnparsedItem;
    use std::result::Result;

    /// Output the whole `m` item value
    #[cfg(feature = "incomplete")] // untested
    #[expect(dead_code)] // will be used when we encode a whole routerstatus
    #[allow(clippy::unnecessary_wraps)]
    pub(crate) fn write_item_value_onto(
        digest: &FixedB64<DOC_DIGEST_LEN>,
        out: ItemEncoder,
    ) -> Result<(), Bug> {
        out.arg(digest);
        Ok(())
    }

    /// Parse the whole `m` item value
    pub(crate) fn from_unparsed(mut item: UnparsedItem) -> Result<FixedB64<DOC_DIGEST_LEN>, EP> {
        item.check_no_object()?;
        FixedB64::from_args(item.args_mut()).map_err(item.args().error_handler("doc_digest"))
    }
}
