//! Misc helper functions and types for use in parsing network documents

use derive_deftly::define_derive_deftly;

pub(crate) mod str;

pub mod batching_split_before;

use std::iter::Peekable;

#[cfg(test)]
use std::fmt::Display;

define_derive_deftly! {
    /// Implement `AsMut<Self>`
    ///
    /// For Reasons, Rust does not have a blanket:
    ///
    /// ```rust,ignore
    /// impl<T> AsMut<T> for T { .. }
    /// ```
    ///
    /// This derive macro expands to the obvious and trivial implementation,
    /// for the type that it's applied to.
    //
    // TODO move this somewhere lower in the stack, eg tor-basic-utils
    export AsMutSelf expect items:

    impl<$tgens> ::std::convert::AsMut<Self> for $ttype where $twheres {
        fn as_mut(&mut self) -> &mut Self {
            self
        }
    }
}

#[cfg(test)]
/// Assert that `$a = $b`; if not, panic with a unidiff
//
// implementation is in fn assert_eq_or_diff, at the bottom of the file
macro_rules! assert_eq_or_diff {
    { $a:expr, $b:expr $(,)? } => {
        assert_eq_or_diff!($a, $b, "")
    };
    { $a:expr, $b:expr , $($message:tt)*} => {
        $crate::util::assert_eq_or_diff(
            &$a,
            stringify!($a),
            &$b,
            stringify!($b),
            &format_args!($($message)*),
        )
    };
}

/// An iterator with a `.peek()` method
///
/// We make this a trait to avoid entangling all the types with `Peekable`.
/// Ideally we would do this with `Itertools::PeekingNext`
/// but that was not implemented for `&mut PeekingNext`
/// when we wrote this code,
/// and we need that because we use a lot of `&mut NetdocReader`.
/// <https://github.com/rust-itertools/itertools/issues/678>
///
/// TODO: As of itertools 0.11.0, `PeekingNext` _is_ implemented for
/// `&'a mut I where I: PeekingNext`, so we can remove this type some time.
///
/// # **UNSTABLE**
///
/// This type is UNSTABLE and not part of the semver guarantees.
/// You'll only see it if you ran rustdoc with `--document-private-items`.
// This is needed because this is a trait bound for batching_split_before.
#[doc(hidden)]
pub trait PeekableIterator: Iterator {
    /// Inspect the next item, if there is one
    fn peek(&mut self) -> Option<&Self::Item>;
}

impl<I: Iterator> PeekableIterator for Peekable<I> {
    fn peek(&mut self) -> Option<&Self::Item> {
        self.peek()
    }
}

impl<I: PeekableIterator> PeekableIterator for &mut I {
    fn peek(&mut self) -> Option<&Self::Item> {
        <I as PeekableIterator>::peek(*self)
    }
}

/// A Private module for declaring a "sealed" trait.
pub(crate) mod private {
    /// A non-exported trait, used to prevent others from implementing a trait.
    ///
    /// For more information on this pattern, see [the Rust API
    /// guidelines](https://rust-lang.github.io/api-guidelines/future-proofing.html#c-sealed).
    #[expect(dead_code, unreachable_pub)] // TODO keep this Sealed trait in case we want it again?
    pub trait Sealed {}
}

#[cfg(test)]
#[allow(unused)]
fn test_as_mut_compiles() {
    use derive_deftly::Deftly;

    #[derive(Deftly)]
    #[derive_deftly(AsMutSelf)]
    struct S<T: Clone>
    where
        Option<T>: Clone,
    {
        t: T,
    }

    let _: &mut S<()> = S { t: () }.as_mut();
}

#[cfg(test)]
pub(crate) fn regsub(update: &mut String, re: &str, repl: impl regex::Replacer) {
    *update = regex::Regex::new(&format!("(?m){re}"))
        .expect(re)
        .replace_all(update, repl)
        .to_string();
}

#[cfg(test)]
pub(crate) fn assert_eq_or_diff(
    a: &str,
    a_what: &str,
    b: &str,
    b_what: &str,
    message: &dyn Display,
) {
    use imara_diff::{Algorithm, BasicLineDiffPrinter, Diff, InternedInput, UnifiedDiffConfig};

    if a == b {
        return;
    }
    let input = InternedInput::new(a, b);
    let mut diff = Diff::compute(Algorithm::Histogram, &input);
    diff.postprocess_lines(&input);
    panic!(
        // rustdoc insists on this unhelpful formatting
        "===== document {a_what} =====
{a}
===== document {b_what} =====
{b}
===== diff ====
{}
===== documents differ: {a_what} != {b_what} =====
{message}
",
        diff.unified_diff(
            &BasicLineDiffPrinter(&input.interner),
            UnifiedDiffConfig::default(),
            &input,
        ),
    );
}
