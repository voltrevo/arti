//! Coarse time types for cheap time operations.
//!
//! This module provides reduced-precision time types that are cheaper to obtain
//! than standard `Instant` on most platforms.
//!
//! On native platforms, this uses `coarsetime` which calls the OS's
//! `CLOCK_MONOTONIC_COARSE`, `CLOCK_MONOTONIC_FAST`, or similar.
//!
//! On WASM, this falls back to the standard `Instant` type since browser
//! `performance.now()` is already relatively cheap.

use std::time;

use derive_more::{Add, AddAssign, Sub, SubAssign};
#[cfg(not(target_arch = "wasm32"))]
use paste::paste;

/// A duration with reduced precision, and, in the future, saturating arithmetic
///
/// This type represents a (nonnegative) period
/// between two [`CoarseInstant`]s.
///
/// This is (slightly lossily) interconvertible with `std::time::Duration`.
///
/// ### Range and precision
///
/// A `CoarseDuration` can represent at least 2^31 seconds,
/// at a granularity of at least 1 second.
///
/// ### Panics
///
/// Currently, operations on `CoarseDuration` (including conversions)
/// can panic on under/overflow.
/// We regard this as a bug.
/// The intent is that all operations will saturate.
#[derive(Copy, Clone, Debug, Ord, PartialOrd, Eq, PartialEq, Hash)] //
#[derive(Add, Sub, AddAssign, SubAssign)]
pub struct CoarseDuration(
    /// The underlying duration representation
    #[cfg(not(target_arch = "wasm32"))]
    coarsetime::Duration,
    /// On WASM, use std::time::Duration directly
    #[cfg(target_arch = "wasm32")]
    time::Duration,
);

/// A monotonic timestamp with reduced precision, and, in the future, saturating arithmetic
///
/// Like `std::time::Instant`, but:
///
///  - [`RealCoarseTimeProvider::now_coarse()`] is cheap on all platforms,
///    unlike `std::time::Instant::now`.
///
///  - **Not true yet**: Arithmetic is saturating (so, it's panic-free).
///
///  - Precision and accuracy are reduced.
///
///  - *Cannot* be compared with, or converted to/from, `std::time::Instant`.
///    It has a completely different timescale to `Instant`.
///
/// You can obtain this (only) from `CoarseTimeProvider::now_coarse`.
///
/// ### Range and precision
///
/// The range of a `CoarseInstant` is not directly visible,
/// since the absolute value isn't.
/// `CoarseInstant`s are valid only within the context of one program execution (process).
///
/// Correct behaviour with processes that run for more than 2^31 seconds (about 30 years)
/// is not guaranteed.
///
/// The precision is no worse than 1 second.
///
/// ### Panics
///
/// Currently, operations on `CoarseInstant` and `CoarseDuration`
/// can panic on under/overflow.
/// We regard this as a bug.
/// The intent is that all operations will saturate.
#[derive(Copy, Clone, Debug, Ord, PartialOrd, Eq, PartialEq, Hash)]
#[cfg(not(target_arch = "wasm32"))]
pub struct CoarseInstant(coarsetime::Instant);

/// On WASM, use crate's Instant since coarsetime doesn't support WASM
#[derive(Copy, Clone, Debug, Ord, PartialOrd, Eq, PartialEq, Hash)]
#[cfg(target_arch = "wasm32")]
pub struct CoarseInstant(crate::Instant);

// ==================== CoarseDuration conversions ====================

#[cfg(not(target_arch = "wasm32"))]
impl From<time::Duration> for CoarseDuration {
    fn from(td: time::Duration) -> CoarseDuration {
        CoarseDuration(td.into())
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl From<CoarseDuration> for time::Duration {
    fn from(cd: CoarseDuration) -> time::Duration {
        cd.0.into()
    }
}

#[cfg(target_arch = "wasm32")]
impl From<time::Duration> for CoarseDuration {
    fn from(td: time::Duration) -> CoarseDuration {
        CoarseDuration(td)
    }
}

#[cfg(target_arch = "wasm32")]
impl From<CoarseDuration> for time::Duration {
    fn from(cd: CoarseDuration) -> time::Duration {
        cd.0
    }
}

// ==================== CoarseInstant arithmetic (native) ====================

/// implement `$AddSub<CoarseDuration> for CoarseInstant`, and `*Assign`
#[cfg(not(target_arch = "wasm32"))]
macro_rules! impl_add_sub { { $($AddSub:ident),* $(,)? } => { paste! { $(
    impl std::ops::$AddSub<CoarseDuration> for CoarseInstant {
        type Output = CoarseInstant;
        fn [< $AddSub:lower >](self, rhs: CoarseDuration) -> CoarseInstant {
            CoarseInstant(self.0. [< $AddSub:lower >]( rhs.0 ))
        }
    }
    impl std::ops::[< $AddSub Assign >]<CoarseDuration> for CoarseInstant {
        fn [< $AddSub:lower _assign >](&mut self, rhs: CoarseDuration) {
            use std::ops::$AddSub;
            *self = self.[< $AddSub:lower >](rhs);
        }
    }
)* } } }

#[cfg(not(target_arch = "wasm32"))]
impl_add_sub!(Add, Sub);

// ==================== CoarseInstant arithmetic (WASM) ====================

#[cfg(target_arch = "wasm32")]
impl std::ops::Add<CoarseDuration> for CoarseInstant {
    type Output = CoarseInstant;
    fn add(self, rhs: CoarseDuration) -> CoarseInstant {
        CoarseInstant(self.0 + time::Duration::from(rhs))
    }
}

#[cfg(target_arch = "wasm32")]
impl std::ops::AddAssign<CoarseDuration> for CoarseInstant {
    fn add_assign(&mut self, rhs: CoarseDuration) {
        *self = *self + rhs;
    }
}

#[cfg(target_arch = "wasm32")]
impl std::ops::Sub<CoarseDuration> for CoarseInstant {
    type Output = CoarseInstant;
    fn sub(self, rhs: CoarseDuration) -> CoarseInstant {
        CoarseInstant(self.0 - time::Duration::from(rhs))
    }
}

#[cfg(target_arch = "wasm32")]
impl std::ops::SubAssign<CoarseDuration> for CoarseInstant {
    fn sub_assign(&mut self, rhs: CoarseDuration) {
        *self = *self - rhs;
    }
}

// ==================== CoarseInstant - CoarseInstant ====================

/// Implement `CoarseInstant - CoarseInstant -> CoarseDuration` (native)
#[cfg(not(target_arch = "wasm32"))]
impl std::ops::Sub<CoarseInstant> for CoarseInstant {
    type Output = CoarseDuration;
    fn sub(self, rhs: CoarseInstant) -> CoarseDuration {
        CoarseDuration(self.0 - rhs.0)
    }
}

/// Implement `CoarseInstant - CoarseInstant -> CoarseDuration` (WASM)
#[cfg(target_arch = "wasm32")]
impl std::ops::Sub<CoarseInstant> for CoarseInstant {
    type Output = CoarseDuration;
    fn sub(self, rhs: CoarseInstant) -> CoarseDuration {
        // crate::Instant subtraction returns std::time::Duration
        CoarseDuration(self.0 - rhs.0)
    }
}

// ==================== CoarseInstant methods ====================

impl CoarseInstant {
    /// Returns the current coarse instant.
    ///
    /// This is a convenience method that calls the underlying platform-specific
    /// coarse time implementation directly. On native platforms, this uses
    /// `coarsetime::Instant::now()`. On WASM, this uses the crate's `Instant::now()`.
    ///
    /// Note: For mockable time in tests, prefer using `CoarseTimeProvider::now_coarse()`
    /// from a runtime instead.
    #[cfg(not(target_arch = "wasm32"))]
    #[inline]
    pub fn now() -> Self {
        CoarseInstant(coarsetime::Instant::now())
    }

    /// Returns the current coarse instant (WASM version).
    #[cfg(target_arch = "wasm32")]
    #[inline]
    pub fn now() -> Self {
        CoarseInstant(crate::Instant::now())
    }

    /// Returns the time elapsed since this instant was created.
    ///
    /// Note: For mockable time in tests, prefer computing elapsed time using
    /// `CoarseTimeProvider::now_coarse()` from a runtime instead.
    #[inline]
    pub fn elapsed(&self) -> CoarseDuration {
        Self::now() - *self
    }
}

// ==================== CoarseTimeProvider trait ====================

/// Trait for providing reduced-precision timestamps
///
/// This trait allows for mockable coarse time in tests while using
/// cheap OS calls in production.
pub trait CoarseTimeProvider: Clone + Send + Sync + 'static {
    /// Return the `CoarseTimeProvider`'s view of the current instant.
    ///
    /// This is supposed to be cheaper than `std::time::Instant::now`.
    fn now_coarse(&self) -> CoarseInstant;
}

// ==================== RealCoarseTimeProvider ====================

/// Provider of reduced-precision timestamps using the real OS clock
///
/// This is a ZST.
#[derive(Default, Clone, Debug)]
#[non_exhaustive]
pub struct RealCoarseTimeProvider {}

impl RealCoarseTimeProvider {
    /// Returns a new `RealCoarseTimeProvider`
    ///
    /// All `RealCoarseTimeProvider`s are equivalent.
    #[inline]
    pub fn new() -> Self {
        RealCoarseTimeProvider::default()
    }
}

impl CoarseTimeProvider for RealCoarseTimeProvider {
    #[inline]
    fn now_coarse(&self) -> CoarseInstant {
        CoarseInstant::now()
    }
}

// ==================== Tests ====================

#[cfg(not(miri))] // coarse_time subtracts with overflow in miri
#[cfg(test)]
mod test {
    // @@ begin test lint list maintained by maint/add_warning @@
    #![allow(clippy::bool_assert_comparison)]
    #![allow(clippy::clone_on_copy)]
    #![allow(clippy::dbg_macro)]
    #![allow(clippy::mixed_attributes_style)]
    #![allow(clippy::print_stderr)]
    #![allow(clippy::print_stdout)]
    #![allow(clippy::single_char_pattern)]
    #![allow(clippy::unwrap_used)]
    #![allow(clippy::unchecked_time_subtraction)]
    #![allow(clippy::useless_vec)]
    #![allow(clippy::needless_pass_by_value)]
    //! <!-- @@ end test lint list maintained by maint/add_warning @@ -->
    #![allow(clippy::erasing_op)]
    use super::*;

    #[test]
    fn basic() {
        let t1 = RealCoarseTimeProvider::new().now_coarse();
        let t2 = t1 + CoarseDuration::from(time::Duration::from_secs(10));
        let t0 = t1 - CoarseDuration::from(time::Duration::from_secs(10));

        assert!(t0 < t1);
        assert!(t0 < t2);
        assert!(t1 < t2);
    }
}
