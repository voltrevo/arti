//! Implementation for microdesc consensus documents.
//
// Read this file in conjunction with `each_variety.rs`.
// See "module scope" ns_variety_definition_macros.rs.

use super::*;

// Import `each_variety.rs`, appropriately variegated
ns_do_variety_md! {}

define_constant_string! {
    /// The `md` keyword in a microdescriptor consensus heading line
    ///
    /// This type is one of the fields in `NetworkStatusVersionItem`.
    ///
    /// md consensuses start with `network-status-version 3 md ...`
    ///
    /// In our terminology this is an `md` consensus, in but the protocol it's `microdesc`
    /// So in *this* variety, the variety it's the fixed string `microdesc`.
    VarietyKeyword = "microdesc";
}
