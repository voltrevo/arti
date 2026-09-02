//! Convenience implementation of a TimeBound object.

use crate::{TimeBound, TimeValidityError};
use itertools::chain;
use std::ops::{Bound, Deref, RangeBounds};
use web_time_compat as time;

/// A `TimeBound` object that is valid for a specified range of time.
///
/// The range is given as an argument, as in `t1..t2`.
///
/// The range is always treated as inclusive.
///
/// **Non-invariant**: it is possible for the start to be after the end.
/// In that case, it's simply never valid: either expired, or too soon, or both.
///
/// `TimeRangeBound<()>` aka `TimeRange` is sometimes used as a representation of a time range,
/// for example, the return value from [`TimeBound::bounds`].
///
/// ```
/// use web_time_compat::{SystemTime, SystemTimeExt, Duration};
/// use tor_checkable::{TimeBound, TimeValidityError, timed::TimeRangeBound};
///
/// let now = SystemTime::get();
/// let one_hour = Duration::new(3600, 0);
///
/// // This seven is only valid for another hour!
/// let seven = TimeRangeBound::new(7_u32, ..now+one_hour);
///
/// assert_eq!(seven.if_valid_at(&now).unwrap(), 7);
///
/// // That consumed the previous seven. Try another one.
/// let seven = TimeRangeBound::new(7_u32, ..now+one_hour);
/// assert_eq!(seven.if_valid_at(&(now+2*one_hour)),
///            Err(TimeValidityError::Expired(one_hour)));
///
/// ```
#[derive(Debug, Clone)]
#[cfg_attr(test, derive(Eq, PartialEq))]
pub struct TimeRangeBound<T> {
    /// The underlying object, which we only want to expose if it is
    /// currently timely.
    obj: T,
    /// If present, when the object first became valid.
    start: Option<time::SystemTime>,
    /// If present, when the object will no longer be valid.
    end: Option<time::SystemTime>,
}

/// Validity time range.
///
/// We use `TimeRangeBound<()>` to represent just a validity range.
//
// We could have a separate `TimeBounds` struct but it would have to have
// many of the same constructors, accessors, etc.
pub type TimeRange = TimeRangeBound<()>;

/// Deprecated compatibility alias for [`TimeRangeBound`]
#[deprecated = "use the new name, TimeRangeBound, instead"]
pub type TimerangeBound<T> = TimeRangeBound<T>;

/// Helper: convert a Bound to its underlying value, if any.
///
/// This helper discards information about whether the bound was
/// inclusive or exclusive.  However, since SystemTime has sub-second
/// precision, we really don't care about what happens when the
/// nanoseconds are equal to exactly 0.
fn unwrap_bound(b: Bound<&'_ time::SystemTime>) -> Option<time::SystemTime> {
    match b {
        Bound::Included(x) => Some(*x),
        Bound::Excluded(x) => Some(*x),
        _ => None,
    }
}

impl<T> TimeRangeBound<T> {
    /// Construct a new TimeRangeBound object from a given object and range.
    ///
    /// Note that we do not distinguish between inclusive and
    /// exclusive bounds: `x..y` and `x..=y` are treated the same
    /// here - as an inclusive range.
    ///
    /// Use `TimeRange::new_range` to create a `TimeRange` aka a `TimeRangeBound<()>`.
    pub fn new<U>(obj: T, range: U) -> Self
    where
        U: RangeBounds<time::SystemTime>,
    {
        let start = unwrap_bound(range.start_bound());
        let end = unwrap_bound(range.end_bound());
        Self { obj, start, end }
    }

    /// Construct a new TimeRangeBound object from a given object, start time, and end time.
    pub fn new_from_start_end(
        obj: T,
        start: Option<time::SystemTime>,
        end: Option<time::SystemTime>,
    ) -> Self {
        Self { obj, start, end }
    }

    /// Adjust this time-range bound to tolerate an initial validity
    /// time farther in the past.
    #[must_use]
    pub fn extend_start_bound(self, d: time::Duration) -> Self {
        let start = match self.start {
            Some(t) => t.checked_sub(d),
            _ => None,
        };
        Self { start, ..self }
    }
    /// Adjust this time-range bound to tolerate an expiration time farther
    /// in the future.
    #[must_use]
    pub fn extend_end_bound(self, d: time::Duration) -> Self {
        let end = match self.end {
            Some(t) => t.checked_add(d),
            _ => None,
        };
        Self { end, ..self }
    }

    /// Deprecated alias for `extend_start_bound`
    #[deprecated = "use extend_start_bound instead"]
    #[must_use]
    pub fn extend_pre_tolerance(self, d: time::Duration) -> Self {
        self.extend_start_bound(d)
    }
    /// Deprecated alias for `extend_end_bound`
    #[deprecated = "use extend_end_bound instead"]
    #[must_use]
    pub fn extend_tolerance(self, d: time::Duration) -> Self {
        self.extend_end_bound(d)
    }

    /// Consume this [`TimeRangeBound`], and return a new one with the same
    /// bounds, applying `f` to its protected value.
    ///
    /// The caller must ensure that `f` does not make any assumptions about the
    /// timeliness of the protected value, or leak any of its contents in
    /// an inappropriate way.
    #[must_use]
    pub fn dangerously_map<F, U>(self, f: F) -> TimeRangeBound<U>
    where
        F: FnOnce(T) -> U,
    {
        TimeRangeBound {
            obj: f(self.obj),
            start: self.start,
            end: self.end,
        }
    }

    /// Consume this TimeRangeBound, and return its underlying time bounds and
    /// object.
    ///
    /// The caller takes responsibility for making sure that the bounds are
    /// actually checked.
    pub fn dangerously_into_parts(self) -> (T, TimeRange) {
        let bounds = self.bounds();

        (self.obj, bounds)
    }

    /// Return a reference to the inner object of this TimeRangeBound, without
    /// checking the time interval.
    ///
    /// The caller takes responsibility for making sure that nothing is actually
    /// done with the inner object that would rely on the bounds being correct, until
    /// the bounds are (eventually) checked.
    pub fn dangerously_peek(&self) -> &T {
        &self.obj
    }

    /// Return a `TimeRangeBound` containing a reference
    ///
    /// This can be useful to call methods like `.check_valid_at`
    /// without consuming the inner `T`.
    pub fn as_ref(&self) -> TimeRangeBound<&T> {
        TimeRangeBound {
            obj: &self.obj,
            start: self.start,
            end: self.end,
        }
    }

    /// Return a `TimeRangeBound` containing a reference to `T`'s `Deref`
    pub fn as_deref(&self) -> TimeRangeBound<&T::Target>
    where
        T: Deref,
    {
        self.as_ref().dangerously_map(|t| &**t)
    }

    /// Return the underlying time bounds of this object.
    pub fn bounds_start_end(&self) -> (Option<time::SystemTime>, Option<time::SystemTime>) {
        (self.start, self.end)
    }

    /// Narrow the bounds of `self` to the overlap with `bounds`
    ///
    /// If the bounds conflict (ie, if the intersection is empty),
    /// simply yields a `TimeRangeBound` that is never valid.
    ///
    /// (This is unlike `tor_basic_utils::rangebounds::RangeBoundsExt::intersect`
    /// which *is* implemented for `TimeRange` via [`RangeBounds`]:
    /// `intersect` insists on returning a well-formed range,
    /// whereas `TimeRangeBound` can be empty if `start > end`.)
    // (we can't make the ref to tor_basic_utils a doc link since that's not in scope!)
    pub fn intersect_bounds(&mut self, bounds: TimeRange) {
        self.start = chain!(self.start, bounds.start()).max();
        self.end = chain!(self.end, bounds.end()).min();
    }

    /// Process multiple `TimeBound`s, intersecting their validity ranges
    ///
    /// Within `logic`, [`TimeBound::unwrap_with`] can be used,
    /// for unwrapping [`TimeBound`]s.
    ///
    /// Those time bounds are accumulated within the [`TimeRangeBoundBuilder`],
    /// and when `logic` returns, they are applied to its result.
    ///
    /// This allows multiple time-bound components of (a Tor protocol element)
    /// to be conveniently processed into an overall return value.
    ///
    /// The API is intended to prevent accidentally forgetting to check
    /// or process one of the time bounds; `TimeRangeBoundBuilder` is
    /// an alternative to manual use of `dangerously_*` and `intersect`.
    ///
    /// # CORRECTNESS
    ///
    /// Everything that needs to be bound to the time range must be returned
    /// only as part of the return value from `logic`.
    ///
    /// It is the caller's responsibility not to smuggle out
    /// values whose validity time has not been checked
    /// out via mutable captures in `logic`, global variables, etc.
    ///
    /// Likewise, if `logic` returns `Err`, this must mean that callers don't treat
    /// the data as valid or successful.  I.e. `Error` must really be an error,
    /// and not be used as a way to smuggle out potentially-out-of-time-range data.
    ///
    /// # Example
    ///
    /// ```
    /// use humantime::parse_rfc3339;
    /// use tor_checkable::{TimeBound as _, TimeRangeBound};
    ///
    /// // Fake document.  A real document would involve signature verification too.
    /// struct Data {}
    /// struct FakeDoc { data: Data, sig: TimeRangeBound<()>, }
    /// impl FakeDoc {
    ///     fn parse(_dummy: &str) -> TimeRangeBound<Self> {
    ///         let t = |s| parse_rfc3339(s).unwrap();
    ///         let sig = TimeRangeBound::new((), ..=t("2001-01-01T00:00:01Z"));
    ///         let doc = FakeDoc { data: Data {}, sig };
    ///         TimeRangeBound::new(doc, ..=t("2000-01-01T00:00:01Z"))
    ///     }
    /// }
    ///
    /// // Demo usage of TimeBoundRangeBuilder, in verification function
    /// fn parse_verify(input: &str) -> Result<TimeRangeBound<Data>, ()> {
    ///     let parsed = FakeDoc::parse(input); // real parser would be fallible
    ///     TimeRangeBound::build_intersect(move |times| {
    ///         let FakeDoc { data, sig } = parsed.unwrap_with(times);
    ///         let _: () = sig.unwrap_with(times); // would verify signature too
    ///         Ok(data)
    ///     })
    /// }
    ///
    /// assert_eq!(
    ///     parse_verify("dummy").unwrap().bounds().end(),
    ///     Some(parse_rfc3339("2000-01-01T00:00:01Z").unwrap()),
    /// );
    /// ```
    pub fn build_intersect<Error, Logic>(logic: Logic) -> Result<Self, Error>
    where
        Logic: FnOnce(&mut TimeRangeBoundBuilder) -> Result<T, Error>,
    {
        let mut builder = TimeRangeBoundBuilder(TimeRange::new_range(..));
        let output = logic(&mut builder)?;
        Ok(builder.0.apply_to(output))
    }
}

impl TimeRange {
    /// Create a new `TimeRange` from a `std::ops::RangeBounds`
    pub fn new_range<U>(range: U) -> Self
    where
        U: RangeBounds<time::SystemTime>,
    {
        Self::new((), range)
    }

    /// Applies this `TimeRange` to a value, protecting it
    pub fn apply_to<T>(self, t: T) -> TimeRangeBound<T> {
        TimeRangeBound::new(t, self.bounds())
    }

    /// Get the start of the validity period
    ///
    /// `None` means there is no start: the object has been valid forever.
    ///
    /// Provided only for `TimeRange`; to call on a general [`TimeRangeBound<T>`],
    /// write `.bounds().start()`.
    pub fn start(&self) -> Option<time::SystemTime> {
        self.start
    }

    /// Get the end of the validity period
    ///
    /// `None` means there is no end: the object will been valid forever.
    /// This is normally a mistake.
    ///
    /// Provided only for `TimeRange`; to call on a general [`TimeRangeBound<T>`],
    /// write `.bounds().end()`.
    //
    // We could forbid the lack of an expiry time,
    // but it would make everything much less consistent.
    pub fn end(&self) -> Option<time::SystemTime> {
        self.end
    }
}

/// Accumulator used by `TimeRangeBounds::build_intersect`
///
/// Provided to the user's `logic` callback by [`TimeRangeBound::build_intersect`]
///
/// There is no other way to obtain a `TimeRangeBoundBuilder`.
// ^ this property allows the API to prevent accidental drops of time bounds.
pub struct TimeRangeBoundBuilder(TimeRange);

impl TimeRangeBoundBuilder {
    /// Handle a `TimeBound`, ensuring its validity range will be honoured
    ///
    /// This is equivalent to [`TimeBound::unwrap_with`],
    /// which is normally more convenient.
    ///
    /// # CORRECTNESS
    ///
    /// See [`TimeBound::unwrap_with`] and [`TimeRangeBound::build_intersect`].
    pub fn incorporate_unwrap<Component: TimeBound>(
        &mut self,
        component: Component,
    ) -> Component::Inner {
        self.intersect_bounds(component.bounds());
        // Correctness: we include the component's bounds in `self`,
        // so that when the whole `build` function returns, those bounds will be re-applied.
        component.dangerously_assume_timely()
    }

    /// Narrow the bounds of `self` to the overlap with `bounds`
    ///
    /// Equivalent to `.as_mut_range().intersect_bounds()`.
    pub fn intersect_bounds(&mut self, bounds: TimeRange) {
        self.as_mut_range().intersect_bounds(bounds);
    }

    /// Mutably access the being-built time range.
    ///
    /// This range is the intersection of all the ranges
    /// from calls to `incorporate_unwrap` and
    /// `intersect_bounds`.
    ///
    /// # CORRECTNESS
    ///
    /// Normally it is only correct to narrow the range, not widen it.
    /// Getting the time range right is the responsibility of the caller.
    ///
    /// Consider [`intersect_bounds`](TimeRangeBoundBuilder::intersect_bounds) instead.
    pub fn as_mut_range(&mut self) -> &mut TimeRange {
        &mut self.0
    }
}

impl<T> RangeBounds<time::SystemTime> for TimeRangeBound<T> {
    fn start_bound(&self) -> Bound<&time::SystemTime> {
        self.start
            .as_ref()
            .map(Bound::Included)
            .unwrap_or(Bound::Unbounded)
    }

    fn end_bound(&self) -> Bound<&time::SystemTime> {
        self.end
            .as_ref()
            .map(Bound::Included)
            .unwrap_or(Bound::Unbounded)
    }
}

/// Implement `From<$R> for TimeRange` via `new_range`
macro_rules! impl_from_range { { $R:ty } => {
    impl From<$R> for TimeRange {
        fn from(r: $R) -> TimeRange {
            TimeRange::new_range(r)
        }
    }
} }

// We don't implement trivial-seeming `From`/`Into` conversions from non-inclusive ranges,
// since strictly speaking we don't preserve the semantics.
// They can still be converted manually with `new_range`.
impl_from_range! { std::ops::RangeFrom<time::SystemTime> }
impl_from_range! { std::ops::RangeFull }
impl_from_range! { std::ops::RangeInclusive<time::SystemTime> }
impl_from_range! { std::ops::RangeToInclusive<time::SystemTime> }

impl<T> crate::TimeBound for TimeRangeBound<T> {
    type Inner = T;

    fn bounds(&self) -> TimeRange {
        TimeRangeBound {
            obj: (),
            start: self.start,
            end: self.end,
        }
    }

    fn check_valid_at(&self, t: &time::SystemTime) -> Result<(), TimeValidityError> {
        use crate::TimeValidityError;
        if let Some(start) = self.start {
            if let Ok(d) = start.duration_since(*t)
                && d > time::Duration::ZERO
            {
                return Err(TimeValidityError::NotYetValid(d));
            }
        }

        if let Some(end) = self.end {
            if let Ok(d) = t.duration_since(end)
                && d > time::Duration::ZERO
            {
                return Err(TimeValidityError::Expired(d));
            }
        }

        Ok(())
    }

    fn dangerously_assume_timely(self) -> T {
        self.obj
    }
}

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
    #![allow(clippy::string_slice)] // See arti#2571
    //! <!-- @@ end test lint list maintained by maint/add_warning @@ -->
    use super::*;
    use crate::{TimeBound, TimeValidityError};
    use humantime::parse_rfc3339;
    use tor_basic_utils::rangebounds::RangeBoundsExt as _;
    use web_time_compat::{Duration, SystemTime, SystemTimeExt};

    #[test]
    fn test_bounds() {
        #![allow(clippy::unwrap_used)]
        let one_day = Duration::new(86400, 0);
        let mixminion_v0_0_1 = parse_rfc3339("2003-01-07T00:00:00Z").unwrap();
        let tor_v0_0_2pre13 = parse_rfc3339("2003-10-19T00:00:00Z").unwrap();
        let cussed_nougat = parse_rfc3339("2008-08-02T00:00:00Z").unwrap();
        let tor_v0_4_4_5 = parse_rfc3339("2020-09-15T00:00:00Z").unwrap();
        let today = parse_rfc3339("2020-09-22T00:00:00Z").unwrap();

        let tr = TimeRangeBound::new((), ..tor_v0_4_4_5);
        assert_eq!(tr.start, None);
        assert_eq!(tr.end, Some(tor_v0_4_4_5));
        assert!(tr.check_valid_at(&mixminion_v0_0_1).is_ok());
        assert!(tr.check_valid_at(&tor_v0_0_2pre13).is_ok());
        assert_eq!(
            tr.check_valid_at(&today),
            Err(TimeValidityError::Expired(7 * one_day))
        );

        let tr = TimeRangeBound::new((), tor_v0_0_2pre13..=tor_v0_4_4_5);
        assert_eq!(tr.start, Some(tor_v0_0_2pre13));
        assert_eq!(tr.end, Some(tor_v0_4_4_5));
        assert_eq!(
            tr.check_valid_at(&mixminion_v0_0_1),
            Err(TimeValidityError::NotYetValid(285 * one_day))
        );
        assert!(tr.check_valid_at(&cussed_nougat).is_ok());
        assert_eq!(
            tr.check_valid_at(&today),
            Err(TimeValidityError::Expired(7 * one_day))
        );

        let tr = tr
            .extend_start_bound(5 * one_day)
            .extend_end_bound(2 * one_day);
        assert_eq!(tr.start, Some(tor_v0_0_2pre13 - 5 * one_day));
        assert_eq!(tr.end, Some(tor_v0_4_4_5 + 2 * one_day));

        let tr = tr
            .extend_start_bound(Duration::MAX)
            .extend_end_bound(Duration::MAX);
        assert_eq!(tr.start, None);
        assert_eq!(tr.end, None);

        let tr = TimeRangeBound::new((), tor_v0_4_4_5..);
        assert_eq!(tr.start, Some(tor_v0_4_4_5));
        assert_eq!(tr.end, None);
        assert_eq!(
            tr.check_valid_at(&cussed_nougat),
            Err(TimeValidityError::NotYetValid(4427 * one_day))
        );
        assert!(tr.check_valid_at(&today).is_ok());
    }

    #[test]
    fn test_checking() {
        // West and East Germany reunified
        let de = humantime::parse_rfc3339("1990-10-03T00:00:00Z").unwrap();
        // Czechoslovakia separates into Czech Republic (Bohemia) & Slovakia
        let cz_sk = humantime::parse_rfc3339("1993-01-01T00:00:00Z").unwrap();
        // European Union created
        let eu = humantime::parse_rfc3339("1993-11-01T00:00:00Z").unwrap();
        // South Africa holds first free and fair elections
        let za = humantime::parse_rfc3339("1994-04-27T00:00:00Z").unwrap();

        // check_valid_at
        let tr = TimeRangeBound::new("Hello world", cz_sk..eu);
        assert!(tr.if_valid_at(&za).is_err());

        let tr = TimeRangeBound::new("Hello world", cz_sk..za);
        assert_eq!(tr.if_valid_at(&eu), Ok("Hello world"));

        // check_valid_now
        #[allow(clippy::disallowed_methods)]
        {
            let tr = TimeRangeBound::new("hello world", de..);
            assert_eq!(tr.if_valid_now(), Ok("hello world"));

            let tr = TimeRangeBound::new("hello world", ..za);
            assert!(tr.if_valid_now().is_err());
        }

        // Now try check_valid_at_opt() api
        let tr = TimeRangeBound::new("hello world", de..);
        #[allow(deprecated)]
        {
            assert_eq!(tr.check_valid_at_opt(None), Ok("hello world"));
            let tr = TimeRangeBound::new("hello world", de..);
            assert_eq!(
                tr.check_valid_at_opt(Some(SystemTime::get())),
                Ok("hello world")
            );
            let tr = TimeRangeBound::new("hello world", ..za);
            assert!(tr.check_valid_at_opt(None).is_err());
        }

        // edge cases
        let tr = TimeRangeBound::new("Hello world", de..eu);
        let nano = Duration::from_nanos(1);
        assert!(tr.check_valid_at(&(de - nano)).is_err());
        assert!(tr.check_valid_at(&de).is_ok());
        assert!(tr.check_valid_at(&(de + nano)).is_ok());
        assert!(tr.check_valid_at(&(eu - nano)).is_ok());
        assert!(tr.check_valid_at(&eu).is_ok());
        assert!(tr.check_valid_at(&(eu + nano)).is_err());
    }

    #[test]
    fn test_dangerous() {
        let t1 = SystemTime::get();
        let t2 = t1 + Duration::from_secs(60 * 525600);
        let tr = TimeRangeBound::new("cups of coffee", t1..=t2);

        assert_eq!(tr.dangerously_peek(), &"cups of coffee");

        let (a, b) = tr.dangerously_into_parts();
        assert_eq!(a, "cups of coffee");
        assert_eq!(b.start(), Some(t1));
        assert_eq!(b.end(), Some(t2));
    }

    #[test]
    fn test_map() {
        let t1 = SystemTime::get();
        let min = Duration::from_secs(60);

        let tb = TimeRangeBound::new(17_u32, t1..t1 + 5 * min);
        let tb = tb.dangerously_map(|v| v * v);
        assert!(tb.check_valid_at(&(t1 + 1 * min)).is_ok());
        assert!(tb.check_valid_at(&(t1 + 10 * min)).is_err());

        let val = tb.if_valid_at(&(t1 + 1 * min)).unwrap();
        assert_eq!(val, 289);
    }

    #[test]
    fn test_as_ref() {
        let t1 = SystemTime::get();
        let min = Duration::from_secs(60);

        let tb1: TimeRangeBound<String> = TimeRangeBound::new("hi".into(), t1..t1 + 5 * min);
        let tb2: TimeRangeBound<&String> = tb1.as_ref();
        let tb3: TimeRangeBound<&str> = tb1.as_deref();
        assert_eq!(tb1, tb2.dangerously_map(|s| s.clone()));
        assert_eq!(tb1, tb3.dangerously_map(|s| s.to_owned()));
    }

    #[test]
    fn test_intersect_bounds() {
        // we use tor-basic-utils's intersect as a reference implementation
        let bounds = || {
            chain!(
                [None],
                (0..=10)
                    .map(|days| {
                        parse_rfc3339("2000-01-01T00:00:01Z").unwrap()
                            + Duration::from_secs(days * 86400)
                    })
                    .map(Some),
            )
        };

        for a_start in bounds() {
            for a_end in bounds() {
                for b_start in bounds() {
                    for b_end in bounds() {
                        let mut a = TimeRange::new_from_start_end((), a_start, a_end);
                        let b = TimeRange::new_from_start_end((), b_start, b_end);
                        let exp = a.intersect(&b).map(TimeRange::new_range);
                        a.intersect_bounds(b);
                        if let Some(exp) = exp {
                            assert_eq!(a, exp);
                        } else {
                            assert!(a.start() > a.end());
                        }
                    }
                }
            }
        }
    }
}
