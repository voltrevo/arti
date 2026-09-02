DEPRECATED: `Timebound`: renamed to `TimeBound`, leaving compatibility alias
DEPRECATED: `TimerangeBound`: renamed to `TimeRangeBound`, leaving compatibility alias
ADDED: `TimeRangeBound` now exported at the top-level, not just in `timed`
DEPRECATED: Use `TimeRangeBound::extend_start_bound` instead of `extend_pre_tolerance`
DEPRECATED: Use `TimeRangeBound::extend_end_bound` instead of `extend_tolerance`
ADDED: `TimeRange` (type alias)
ADDED: `TimeRange::new_range`, `apply_to`, `start`, `end`, `intersect_bounds`
ADDED: `TimeRange` `From` impls from (inclusive) `std::ops::Range*` types
BREAKING: `TimeBound` overhauled: new `bounds` method; `Error` removed
BREAKING: `TimeBound::is_valid_at` is now provided and should not generally be overridden
BREAKING: `TimeBound`'s wrapped type is now `TimeBound::Inner`
DEPRECATED: `TimeBound:::check_valid_at_opt`
BREAKING: Use `TimeBound:::if_valid_at` instead of `check_valid_at`
BREAKING: Use `TimeBound:::if_valid_now` instead of `check_valid_now`
BREAKING: Use `TimeBound:::check_valid_at` instead of `is_valid_at`
ADDED: `TimeRangeBound:::build_intersect`, etc.
