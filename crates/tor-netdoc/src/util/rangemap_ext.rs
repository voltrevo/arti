//! Read-modify-write values for a contiguous range of keys, in a range map
//!
//! Separate module mostly so that the tests have somewhere convenient to live.
//!
//! Arguably this should be in another crate.
//! But it brings in rangemap as a dependency which seems undesirable for tor-basic-utils.
//! Right now it is here because this is where its (lowest in stack) call site will be.

use std::ops::{Bound, RangeInclusive};

use itertools::{Itertools, chain};
use rangemap::{RangeInclusiveMap, StepFns, StepLite};

use tor_basic_utils::rangebounds::RangeBoundsExt;

/// Read-modify-write values for a contiguous range of keys, in a range map
///
/// Since the map might contain different values for various parts of the specified range,
/// multiple possible old values might need to be handled.
/// `rangemap_mutate_range` is suitable if different old values can be handled independently,
/// or sequentially.
///
/// Calls `update` for every range currently in the map overlapping with `range0`,
/// and for every gap overlapping with `range`.
///
/// If the mutated value is equal (`PartialEq`), no actual update is made.
///
/// If `update` throws `Err`, any mutations to its first argument *will* be stored in the map,
/// but no further calls to `update` will be made (so only part of the range
/// might be updated).
///
/// `update` should probably not use the provided `&RangeInclusive` argument as an input
/// to calculating how to update the `&mut V`.  Doing so would make the results depend
/// on the details of range fragmentation in the rangemap,
/// which would be inconsistent with the usual use of a rangemap as an optimisation
/// of an abstract data structure which stores a separate value for each key.
///
/// Note that `update` doesn't get a mutable reference *into the map*.
/// so if it mutates its argument and then panics, the update might not be applied.
// RangeInclusiveMap doesn't provide any in-place update API, and an in-place update is also
// incompatible with passing `&mut Option<V>`.
//
// Other APIs that were considered, include:
//
//  * FnMut(Option<V>) -> Option<V>
//
//    This seems less idiomatic.
//
//  * Don't coalesce equal values, skipping a comparison.
//
//    `RangeInclusiveMap` already coalesces adjacent identical ranges.
//    Comparing the old and new value is O(sizeof(V)), whereas RangeInclusiveMap::insert
//    is at least O(log N) and will often involves that comparison against adjacent ranges.
//
//    In theory this optimisation might be done by `RangeInclusiveMap` already, but it's
//    not mentioned.  Probably, Eq is quite cheap.
pub fn rangemap_mutate_range<K, V, StepFnsT, E>(
    map: &mut RangeInclusiveMap<K, V, StepFnsT>,
    range0: &RangeInclusive<K>,
    mut update: impl FnMut(&mut Option<V>, &RangeInclusive<K>) -> Result<(), E>,
) -> Result<(), E>
where
    K: Ord + Clone + StepLite,
    V: PartialEq + Clone,
    StepFnsT: StepFns<K>,
{
    let relevants = chain!(
        map.overlapping(range0).map(|(k, _v)| k.clone()),
        map.gaps(range0),
    )
    .collect_vec();

    for relevant in relevants {
        // `relevant` is a range stored in the rangemap, or a gap, which overlaps with `range0`,
        // but which might be bigger or smaller (or both) than `range0`.
        //
        // Insofar as it's smaller, then if there are other ranges in the rangemap with overlap,
        // they'll be handled separately, in other loop iterations.
        //
        // But insofar as it's bigger, we need to trim it down, because we're only supposed
        // to modify `range0`.
        //
        // We distinguish `k`, the range we are updating in this iteration,
        // from `range0`, the overall range from our caller.
        let k = {
            let k = relevant
                .intersect(range0)
                .expect("intersection of overlapping ranges was empty");

            // RangeBoundsExt::intersect works at the level of the RangeBounds trait,
            // returning `Option<(Bound, Bound)>`,  This helper closure converts one of the
            // returned `Bound`s back to a plain value.  The returned intersection will
            // always be an inclusive range because the input ranges were inclusive
            // (ie, closed, in set theory terms) and intersection preserves set closedness.
            let fix = |b: Bound<&K>| match b {
                Bound::Included(y) => y.clone(),
                _other => unreachable!("intersection of closed ranges wasn't closed"),
            };

            fix(k.0)..=fix(k.1)
        };

        let v0 = map.get(k.start());
        let mut v = v0.cloned();
        // avoid losing an update if Err is returned, so park any `Err` in `r`
        let r = update(&mut v, &k);
        if v.as_ref() != v0 {
            if let Some(v) = v {
                map.insert(k, v);
            } else {
                map.remove(k);
            }
        }
        r?;
    }
    Ok(())
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
    use educe::Educe;
    use std::fmt::Debug;
    use void::{ResultVoidExt as _, Void};

    type Range = RangeInclusive<u8>;
    const ALL_K: Range = 0..=255;
    type Id = u32;

    /// Magic value
    ///
    /// Comparisons use only the value in `v`, but we track its value identity,
    /// which lets us see (for example) whether an "equal" update did anything.
    #[derive(Debug, Clone, Educe)]
    #[educe(PartialEq)]
    struct Value {
        v: char,
        #[educe(PartialEq(ignore))]
        id: Id,
    }

    /// Test wrapper for `RangeInclusiveMap`
    ///
    /// Maintains a separate copy of the expected current V for each K, in `reference`.
    /// Cross-checks it.
    #[derive(Debug, Clone, Educe)]
    #[educe(Default, PartialEq)]
    struct TestState {
        map: RangeInclusiveMap<u8, Value>,
        #[educe(Default(expression = "[None; _]"))] // Only short arrays are Default :-/
        reference: [Option<char>; 256],
        #[educe(PartialEq(ignore))]
        ids: IdGenerator,
    }

    /// avoids constant repetition of `let ; += 1;` pattern
    #[derive(Debug, Clone, Default, PartialEq, Eq)]
    struct IdGenerator(Id);

    impl IdGenerator {
        fn next(&mut self) -> Id {
            let r = self.0;
            self.0 += 1;
            r
        }
    }

    impl TestState {
        fn from_iter(elems: impl IntoIterator<Item = (Range, char)>) -> Self {
            let mut self_ = TestState::default();
            let id = self_.ids.next();
            for (range, v) in elems {
                self_.map.insert(range.clone(), Value { v, id });
                for k in range.clone() {
                    self_.reference[k as usize] = Some(v);
                }
            }
            self_
        }

        /// Calls `rangemap_mutate_range`, but also updates `reference`, makes some checks, etc.
        fn mutate_range<E: Debug>(
            &mut self,
            range0: Range,
            mut real_update: impl FnMut(&Range, &mut Option<Value>) -> Result<(), E>,
        ) -> Result<(), E> {
            let mut updated = [false; 256];
            println!("updating range0={range0:?}");

            let r = rangemap_mutate_range(
                &mut self.map,
                &range0,
                |value: &mut Option<Value>, range| {
                    println!("updating range0={range:?} range={range:?}");
                    assert!(range0.contains(range.start()), "uncontained start");
                    assert!(range0.contains(range.end()), "uncontained end");
                    let r = real_update(range, value);
                    println!("updating range0={range0:?} range={range:?}, to {value:?}, r={r:?}");
                    for k in range.clone() {
                        updated[k as usize] = true;
                        self.reference[k as usize] = value.as_ref().map(|value| value.v);
                    }
                    r
                },
            );
            self.check();

            if r.is_ok() {
                for k in ALL_K {
                    assert_eq!(
                        updated[k as usize],
                        range0.contains(&k),
                        "updated inconsistency k={k:?}",
                    );
                }
            }

            r
        }

        fn set_range(&mut self, range0: Range, val: char, expected_old_values: &str) {
            let id = self.ids.next();
            self.mutate_range(range0.clone(), |k, vmut| {
                assert!(
                    expected_old_values.contains(vmut.as_ref().map(|v| v.v).unwrap_or('_')),
                    "{range0:?} {k:?} {val:?} {vmut:?} {expected_old_values:?}"
                );
                *vmut = Some(Value { v: val, id });
                Ok::<_, Void>(())
            })
            .void_unwrap();
        }

        fn check(&self) {
            for k in ALL_K {
                assert_eq!(
                    self.map.get(&k).map(|v| v.v),
                    self.reference[k as usize],
                    "map now implies wrong v at k={k:?}",
                );
            }
        }
    }

    #[test]
    fn mutations() {
        let s0 = TestState::from_iter([
            //
            (0..=9, 'a'),
            (20..=29, 'x'),
        ]);

        {
            // mutate precisely an existing range
            let mut s = s0.clone();
            s.set_range(0..=9, 'b', "a");
        }
        {
            // mutate precisely a gap
            let mut s = s0.clone();
            s.mutate_range(10..=19, |_k, v| {
                assert_eq!(*v, None);
                Ok::<_, Void>(())
            })
            .void_unwrap();
            assert_eq!(s, s0);
            s.set_range(10..=19, 'n', "_");
        }
        {
            // mutate strictly a subset of an existing range
            let mut s = s0.clone();
            s.set_range(1..=8, 'b', "a");
            // now mutate parts of several ranges, with no gap in between, including a singleton
            s.set_range(7..=9, 'c', "ab");
        }
        {
            // mutate parts of several ranges, with a gap in between
            let mut s = s0.clone();
            s.set_range(5..=25, 'm', "_ax");
        }
        {
            // mutate parts of several ranges, throwing and error halfway through
            let mut s = s0.clone();
            s.mutate_range(5..=25, |k, v| {
                *v = Some(Value { v: 'm', id: 1000 });
                (*k.start() < 15).then_some(()).ok_or(())
            })
            .expect_err("terminated early");
        }
        {
            let mut s = s0.clone();
            let range = 0..=9;
            // overwrite the whole range with the same value
            // if the underlying insert call were made,
            // the value in the range would be replaced
            s.set_range(range.clone(), 'a', "a");
            // check that the thing at 1, the start of the "mutated" range,
            // is in fact *not* the value we inserted but the existing one.
            // this checks that our own PartialEq skip is workign
            let ent = s.map.get(&1).unwrap();
            assert_eq!(ent.id, 0);
            // check that our assumption about rangemap is true
            let id = s.ids.next();
            s.map.insert(range.clone(), Value { v: 'a', id });
            let ent = s.map.get(&1).unwrap();
            assert_eq!(ent.id, id);
        }
    }
}
