//! What a test does when an input outside the repository is absent.
//!
//! nextest counts a test that returns early as passed, so a test that skips
//! because its input is missing reads exactly like one that checked something.
//! Three JIT-vs-interpreter differential tests printed "skipping" and returned
//! on every machine, CI included, and so had never run.
//!
//! Inputs fall into three kinds, and only the first comes here:
//!
//! - **External**: a tool or platform facility the repository cannot carry
//!   (z3, python3, the Swift demangler). CI installs it where the test runs
//!   and sets [`REQUIRE_ENV`]; there, absence is a failure. Elsewhere the test
//!   says why and returns.
//! - **Checked in** (`utils/`, `testdata/`): absence is a broken checkout, so
//!   the test `expect`s it and never skips.
//! - **Local data only** (a benchmark corpus, gitignored sources): the test is
//!   `#[ignore = "..."]`, so nextest reports it as ignored, and panics when an
//!   opt-in run cannot find the input.

/// Set to `1` by CI jobs that provide every external test input.
pub const REQUIRE_ENV: &str = "FISSION_REQUIRE_TEST_INPUTS";

/// Whether this run promised every external input ([`REQUIRE_ENV`]` = 1`).
#[must_use]
pub fn required() -> bool {
    std::env::var(REQUIRE_ENV).is_ok_and(|v| v == "1")
}

/// Report an absent external input: a panic when [`required`], otherwise a
/// message on stderr. The caller returns afterwards.
///
/// ```no_run
/// # fn z3_path() -> Option<std::path::PathBuf> { None }
/// let Some(z3) = z3_path() else {
///     fission_core::test_inputs::absent("a z3 binary (set FISSION_Z3)");
///     return;
/// };
/// ```
///
/// # Panics
///
/// When [`REQUIRE_ENV`] is `1`.
pub fn absent(what: &str) {
    assert!(
        !required(),
        "{REQUIRE_ENV}=1 but this test's input is absent: {what}"
    );
    eprintln!("skipping: {what} is absent (set {REQUIRE_ENV}=1 to make this a failure)");
}
