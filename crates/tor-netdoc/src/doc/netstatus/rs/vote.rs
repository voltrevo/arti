//! Implementation for the style of router status entries used in
//! old-style "ns" consensus documents.
//
// Read this file in conjunction with `each_variety.rs`.
// See "module scope" ns_variety_definition_macros.rs.

use super::*;

// Import `each_variety.rs`, appropriately variegated
ns_do_variety_vote! {}

pub(crate) use crate::doc::routerdesc::{DOC_DIGEST_LEN, RdDigest as DocDigest};
