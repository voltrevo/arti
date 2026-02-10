#![cfg_attr(docsrs, feature(doc_cfg))]
//! Cross-platform time types and utilities for Tor.
//!
//! This crate provides `SystemTime` and `Instant` types that work on both
//! native platforms and WASM, plus minimal utility functions for time conversions.
//!
//! # Platform Compatibility
//!
//! On native platforms, the time types are re-exports of `std::time` types.
//! On WASM, they are `web_time` implementations that work in browser environments.
//!
//! # Usage
//!
//! ```
//! use tor_time::{SystemTime, Instant, Duration, UNIX_EPOCH};
//!
//! // Get current time
//! let now = SystemTime::now();
//! let instant = Instant::now();
//!
//! // Format for display
//! let formatted = tor_time::format_rfc3339(now);
//! ```

// @@ begin lint list maintained by maint/add_warning @@
#![allow(renamed_and_removed_lints)] // @@REMOVE_WHEN(ci_arti_stable)
#![allow(unknown_lints)] // @@REMOVE_WHEN(ci_arti_nightly)
#![warn(missing_docs)]
#![warn(noop_method_call)]
#![warn(unreachable_pub)]
#![warn(clippy::all)]
#![deny(clippy::await_holding_lock)]
#![deny(clippy::cargo_common_metadata)]
#![deny(clippy::cast_lossless)]
#![deny(clippy::checked_conversions)]
#![warn(clippy::cognitive_complexity)]
#![deny(clippy::debug_assert_with_mut_call)]
#![deny(clippy::exhaustive_enums)]
#![deny(clippy::exhaustive_structs)]
#![deny(clippy::expl_impl_clone_on_copy)]
#![deny(clippy::fallible_impl_from)]
#![deny(clippy::implicit_clone)]
#![deny(clippy::large_stack_arrays)]
#![warn(clippy::manual_ok_or)]
#![deny(clippy::missing_docs_in_private_items)]
#![warn(clippy::needless_borrow)]
#![warn(clippy::needless_pass_by_value)]
#![warn(clippy::option_option)]
#![deny(clippy::print_stderr)]
#![deny(clippy::print_stdout)]
#![warn(clippy::rc_buffer)]
#![deny(clippy::ref_option_ref)]
#![warn(clippy::semicolon_if_nothing_returned)]
#![warn(clippy::trait_duplication_in_bounds)]
#![deny(clippy::unchecked_time_subtraction)]
#![deny(clippy::unnecessary_wraps)]
#![warn(clippy::unseparated_literal_suffix)]
#![deny(clippy::unwrap_used)]
#![deny(clippy::mod_module_files)]
#![allow(clippy::let_unit_value)] // This can reasonably be done for explicitness
#![allow(clippy::uninlined_format_args)]
#![allow(clippy::significant_drop_in_scrutinee)] // arti/-/merge_requests/588/#note_2812945
#![allow(clippy::result_large_err)] // temporary workaround for arti#587
#![allow(clippy::needless_raw_string_hashes)] // complained-about code is fine, often best
#![allow(clippy::needless_lifetimes)] // See arti#1765
#![allow(mismatched_lifetime_syntaxes)] // temporary workaround for arti#2060
//! <!-- @@ end lint list maintained by maint/add_warning @@ -->

mod coarse_time;
mod atomic_opt_ts;

// Re-export web_time types (std::time on native, web_time impl on WASM)
pub use web_time::{Duration, Instant, SystemTime, SystemTimeError, UNIX_EPOCH};

// Re-export coarse time types
pub use coarse_time::{CoarseDuration, CoarseInstant, CoarseTimeProvider, RealCoarseTimeProvider};

pub use atomic_opt_ts::AtomicOptTimestamp;

pub mod serde_time;

/// Format a `SystemTime` as an RFC3339 string (cross-platform).
///
/// This function provides consistent time formatting across native and WASM platforms
/// using the `time` crate internally.
///
/// # Returns
///
/// An RFC3339 formatted string (e.g., "2024-01-15T10:30:00Z").
/// Returns `"<time format error>"` if formatting fails.
///
/// # Example
///
/// ```
/// use tor_time::{SystemTime, format_rfc3339};
///
/// let now = SystemTime::now();
/// let formatted = format_rfc3339(now);
/// // Result: something like "2024-01-15T10:30:00Z"
/// ```
pub fn format_rfc3339(t: SystemTime) -> String {
    use time::{format_description::well_known::Rfc3339, OffsetDateTime};
    let secs = t
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_secs();
    OffsetDateTime::from_unix_timestamp(secs as i64)
        .unwrap_or(OffsetDateTime::UNIX_EPOCH)
        .format(&Rfc3339)
        .unwrap_or_else(|_| "<time format error>".into())
}

/// Convert `time::Duration` to `std::time::Duration`.
///
/// The `time` crate's `Duration` can be negative (useful for configuration values
/// parsed from strings), but `std::time::Duration` cannot be negative.
///
/// # Returns
///
/// The equivalent `std::time::Duration`, clamped to `Duration::ZERO` for negative values.
///
/// # Example
///
/// ```
/// use tor_time::time_duration_to_std;
///
/// let positive = time::Duration::seconds(60);
/// let negative = time::Duration::seconds(-60);
///
/// assert_eq!(time_duration_to_std(positive), std::time::Duration::from_secs(60));
/// assert_eq!(time_duration_to_std(negative), std::time::Duration::ZERO);
/// ```
pub fn time_duration_to_std(d: time::Duration) -> std::time::Duration {
    if d.is_negative() {
        return std::time::Duration::ZERO;
    }
    let secs = d.whole_seconds() as u64;
    let nanos = d.subsec_nanoseconds() as u32;
    std::time::Duration::new(secs, nanos)
}

/// Format a `SystemTime` as an HTTP date string (cross-platform).
///
/// This wraps `httpdate::fmt_http_date`, handling the type conversion
/// needed on WASM where `SystemTime` differs from `std::time::SystemTime`.
///
/// # Example
///
/// ```
/// use tor_time::{SystemTime, fmt_http_date};
///
/// let now = SystemTime::now();
/// let formatted = fmt_http_date(now);
/// // Result: something like "Thu, 15 Feb 2024 10:30:00 GMT"
/// ```
#[cfg(not(target_arch = "wasm32"))]
#[inline]
pub fn fmt_http_date(t: SystemTime) -> String {
    // On native, web_time::SystemTime == std::time::SystemTime
    httpdate::fmt_http_date(t)
}

/// Format a `SystemTime` as an HTTP date string (cross-platform).
///
/// This wraps `httpdate::fmt_http_date`, handling the type conversion
/// needed on WASM where `SystemTime` differs from `std::time::SystemTime`.
#[cfg(target_arch = "wasm32")]
#[inline]
pub fn fmt_http_date(t: SystemTime) -> String {
    // On WASM, convert via duration-since-epoch
    let duration = t.duration_since(UNIX_EPOCH).unwrap_or(Duration::ZERO);
    let std_time = std::time::SystemTime::UNIX_EPOCH + duration;
    httpdate::fmt_http_date(std_time)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_rfc3339_unix_epoch() {
        let formatted = format_rfc3339(UNIX_EPOCH);
        assert_eq!(formatted, "1970-01-01T00:00:00Z");
    }

    #[test]
    fn test_format_rfc3339_known_time() {
        // 1705315800 seconds since epoch = 2024-01-15T10:50:00Z
        let time = UNIX_EPOCH + Duration::from_secs(1705315800);
        let formatted = format_rfc3339(time);
        assert_eq!(formatted, "2024-01-15T10:50:00Z");
    }

    #[test]
    fn test_time_duration_to_std_positive() {
        let d = time::Duration::seconds(60);
        assert_eq!(time_duration_to_std(d), std::time::Duration::from_secs(60));
    }

    #[test]
    fn test_time_duration_to_std_negative() {
        let d = time::Duration::seconds(-60);
        assert_eq!(time_duration_to_std(d), std::time::Duration::ZERO);
    }

    #[test]
    fn test_time_duration_to_std_with_nanos() {
        let d = time::Duration::new(5, 123_456_789);
        let std_d = time_duration_to_std(d);
        assert_eq!(std_d.as_secs(), 5);
        assert_eq!(std_d.subsec_nanos(), 123_456_789);
    }

    #[test]
    fn test_time_duration_to_std_zero() {
        let d = time::Duration::ZERO;
        assert_eq!(time_duration_to_std(d), std::time::Duration::ZERO);
    }
}
