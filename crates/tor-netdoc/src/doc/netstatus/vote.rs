//! Implementation for plain consensus documents.
//
// Read this file in conjunction with `each_variety.rs`.
// See "module scope" ns_variety_definition_macros.rs.

use super::*;

// Import `each_variety.rs`, appropriately variegated
ns_do_variety_vote! {}

/// The forbidden flavor keyword in a vote consensus heading line
///
/// This type is one of the fields in `NetworkStatusVersionItem`.
///
/// Votes start with `network-status-version 3`
/// and aren't allowed to have a variety.
///
/// So in *this* variety, we insist that there are no more arguments.
///
/// See also torspec#359.
pub type VarietyKeyword = NoMoreArguments;
