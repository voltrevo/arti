//! Helper functions for writing redacted strings.

use std::fmt;

/// Write up to `chars` _characters_ from the start of `input` onto `f`.
///
/// If any characters are removed, replace them with `ellipsis`.
pub fn write_start_redacted(
    f: &mut fmt::Formatter,
    input: &str,
    chars: usize,
    ellipsis: &str,
) -> fmt::Result {
    if let Some((pos, _)) = input.char_indices().nth(chars) {
        let slice = input
            .get(..pos)
            .expect("Mismatched character offset calculation");
        write!(f, "{slice}{ellipsis}")
    } else {
        write!(f, "{input}")
    }
}

/// Write up to `chars`  _characters_ from the end of `input` onto `f`.
///
/// If any characters are removed, replace them with `ellipsis`.
pub fn write_end_redacted(
    f: &mut fmt::Formatter,
    input: &str,
    chars: usize,
    ellipsis: &str,
) -> fmt::Result {
    if chars == 0 {
        if input.is_empty() {
            Ok(())
        } else {
            write!(f, "{ellipsis}")
        }
    } else if let Some((pos, _)) = input.char_indices().nth_back(chars - 1)
        && pos != 0
    {
        let slice = input
            .get(pos..)
            .expect("Mismatched character offset calculation");
        write!(f, "{ellipsis}{slice}")
    } else {
        write!(f, "{input}")
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

    struct Fmt<'a> {
        string: &'a str,
        n: usize,
        start: bool,
    }

    impl<'a> fmt::Display for Fmt<'a> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            if self.start {
                write_start_redacted(f, self.string, self.n, "…")
            } else {
                write_end_redacted(f, self.string, self.n, "…")
            }
        }
    }

    fn rstart(string: &str, n: usize) -> String {
        Fmt {
            string,
            n,
            start: true,
        }
        .to_string()
    }

    fn rend(string: &str, n: usize) -> String {
        Fmt {
            string,
            n,
            start: false,
        }
        .to_string()
    }

    #[test]
    fn test_redact_start() {
        assert_eq!(&rstart("hello world", 2), "he…");
        assert_eq!(&rstart("he", 2), "he");
        assert_eq!(&rstart("h", 2), "h");
        assert_eq!(&rstart("", 2), "");

        assert_eq!(&rstart("", 0), "");
        assert_eq!(&rstart("hello", 0), "…");

        assert_eq!(&rstart("分久必合，合久必分", 2), "分久…");
        assert_eq!(&rstart("分久必合，合久必分", 4), "分久必合…");
        assert_eq!(&rstart("分久必合，合久必分", 9), "分久必合，合久必分");
        assert_eq!(&rstart("分久必合，合久必分", 10), "分久必合，合久必分");
        assert_eq!(&rstart("分久必合，合久必分", 0), "…");
    }

    #[test]
    fn test_redact_end() {
        assert_eq!(&rend("hello world", 2), "…ld");
        assert_eq!(&rend("he", 2), "he");
        assert_eq!(&rend("h", 2), "h");
        assert_eq!(&rend("", 2), "");

        assert_eq!(&rend("", 0), "");
        assert_eq!(&rend("hello", 0), "…");

        assert_eq!(&rend("分久必合，合久必分", 2), "…必分");
        assert_eq!(&rend("分久必合，合久必分", 4), "…合久必分");
        assert_eq!(&rend("分久必合，合久必分", 9), "分久必合，合久必分");
        assert_eq!(&rend("分久必合，合久必分", 10), "分久必合，合久必分");
        assert_eq!(&rend("分久必合，合久必分", 0), "…");
    }
}
