//! Implementation for plain consensus documents.
//
// Read this file in conjunction with `each_variety.rs`.
// See "module scope" ns_variety_definition_macros.rs.

use super::*;

// Import `each_variety.rs`, appropriately variegated
ns_do_variety_plain! {}

/// The optional `ns` keyword in a plain consensus heading line
///
/// This type is one of the fields in `NetworkStatusVersionItem`.
///
/// plain consensuses start with `network-status-version 3 ns ...`,
/// or are just `network-status-version 3`.
///
/// C Tor doesn't emit `ns`, but we will.
///
/// In our terminology this is a `plain` consensus, in but the protocol it's `ns`.
/// So in *this* variety, we parse as an *optional* fixed string `ns`,
/// and encode as (always) the string `ns`.
///
/// See also torspec#359
//
// This is not Option<fixed string> because we don't actually want to store whether
// the keyword is present.
//
// TODO DIRAUTH Arti consensus method should define that this field is present.
// <https://gitlab.torproject.org/tpo/core/torspec/-/merge_requests/481#note_3409590>
#[derive(Clone, Debug, Default, Eq, PartialEq, Ord, PartialOrd, Hash)]
#[allow(clippy::exhaustive_structs)]
pub struct VarietyKeyword;

/// The constant keyword string
const VARIETY_KEYWORD: &str = "ns";

impl ItemArgument for VarietyKeyword {
    fn write_arg_onto(&self, out: &mut ItemEncoder<'_>) -> StdResult<(), Bug> {
        out.add_arg(&VARIETY_KEYWORD);
        Ok(())
    }
}

impl ItemArgumentParseable for VarietyKeyword {
    fn from_args<'s>(args: &mut ArgumentStream<'s>) -> StdResult<Self, ArgumentError> {
        match args.next() {
            None => Ok(VarietyKeyword),
            Some(s) if s == VARIETY_KEYWORD => Ok(VarietyKeyword),
            Some(_other) => Err(ArgumentError::Invalid),
        }
    }
}
