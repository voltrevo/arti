//! Helpers for [`fs-mistrust`](fs_mistrust) configuration.

use extend::ext;
use fs_mistrust::{Mistrust, MistrustBuilder};

use crate::ConfigBuildError;

/// The environment variable we look at when deciding whether to disable FS permissions checking.
pub const FS_PERMISSIONS_CHECKS_DISABLE_VAR: &str = "ARTI_FS_DISABLE_PERMISSION_CHECKS";

/// Extension trait for `MistrustBuilder` to convert the error type on
/// build.
#[ext(name = BuilderExt)]
pub impl MistrustBuilder {
    /// Run this builder and convert its error type (if any)
    fn build_for_arti(&self) -> Result<Mistrust, ConfigBuildError> {
        self.clone()
            .controlled_by_env_var_if_not_set(FS_PERMISSIONS_CHECKS_DISABLE_VAR)
            .build()
            .map_err(|e| ConfigBuildError::Invalid {
                field: "permissions".to_string(),
                problem: e.to_string(),
            })
    }
}
