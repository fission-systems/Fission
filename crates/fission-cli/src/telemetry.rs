//! Opt-in error reporting to Sentry, compiled only with the `sentry` feature.
//!
//! Nothing is sent unless `FISSION_SENTRY=1` and `SENTRY_DSN` are both set.
//!
//! # Policy: no free text leaves the process
//!
//! Fission analyzes untrusted binaries, so any formatted string (a panic
//! message, a `tracing` message or field, an error's `Display`) may carry
//! bytes, strings or symbol names from the analyzed image, or paths and
//! credentials from the operator's machine. A blacklist scrub cannot tell
//! those apart from harmless text, so this module does not try. An event is
//! built from typed, compile-time facts only, and [`scrub`] decides what an
//! event can carry:
//!
//! - the exception type ([`ReportKind`]: `panic`, `tracing_error`,
//!   `internal_defect`) and mechanism;
//! - the callsite as `file:line`, from `PanicHookInfo::location`,
//!   `tracing::Metadata::{file, line}` or `Location::caller`, when the file
//!   is Fission's own source (`crates/fission-*/...`), the standard library
//!   (`library/...`) or a crates.io dependency (`deps/<crate>-<ver>/...`);
//! - stack frames: the function name when its crate is `fission_*`, the
//!   standard library's or Rayon's (others become `<other>`), `in_app`, and
//!   file and line under the same rule (release builds are stripped, so they
//!   usually carry no symbols);
//! - fingerprint `[exception type, location]`, the `location` tag (the only
//!   tag), level, timestamp and event id;
//! - release (`fission-cli@<version>`), environment, platform and SDK name
//!   only from fixed forms or lists, and the SDK version.
//!
//! Panic payloads and tracing messages/fields are never read; a message,
//! log entry or exception value is replaced by a fixed template. Everything
//! else is dropped.

use sentry::protocol::{
    ClientSdkInfo, Event, Exception, Frame, Level, LogEntry, Mechanism, Stacktrace, Values,
};
use std::borrow::Cow;
use std::collections::BTreeSet;
use std::panic::PanicHookInfo;
use std::sync::Mutex;
use std::time::Duration;

/// Keeps the Sentry client alive; dropping it flushes queued events.
pub type Guard = sentry::ClientInitGuard;

/// Distinct `[kind, location]` groups reported per process; later events in
/// an already-reported group, and every event past the cap, are dropped.
const MAX_DISTINCT_REPORTS: usize = 64;

/// Frames kept per stack trace (innermost first are the useful ones).
const MAX_FRAMES: usize = 64;

/// What replaces a message or log entry; exception values use a template.
const REDACTED: &str = "[redacted]";
/// The only tag an event keeps: the callsite, set by [`callsite_event`].
const LOCATION_TAG: &str = "location";
/// Fingerprint part for an event without a clean location.
const NO_LOCATION: &str = "<unknown location>";
/// Exception type of an event no [`ReportKind`] produced (never sent).
const UNKNOWN_TYPE: &str = "unknown";
/// Function name of a frame outside the reportable crates.
const OTHER_FRAME: &str = "<other>";
/// Crates besides `fission_*` whose function names a frame may carry.
const FRAME_CRATES: &[&str] = &[
    "std",
    "core",
    "alloc",
    "panic_unwind",
    "panic_abort",
    "rayon",
    "rayon_core",
];
/// First component of every path [`clean_source_path`] returns.
const SOURCE_ROOTS: &[&str] = &["crates", "library", "deps"];
/// Fission's own crates, as a workspace-relative prefix.
const OWN_CRATES: &str = "crates/fission-";
const RELEASE_PREFIX: &str = "fission-cli@";
const PLATFORMS: &[&str] = &["native", "rust", "other"];
const ENVIRONMENTS: &[&str] = &[
    "production",
    "development",
    "staging",
    "ci",
    "test",
    "local",
];
const SDK_NAMES: &[&str] = &["sentry.rust"];

/// The only exception types an event may carry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ReportKind {
    Panic,
    TracingError,
    InternalDefect,
}

impl ReportKind {
    const ALL: [ReportKind; 3] = [Self::Panic, Self::TracingError, Self::InternalDefect];

    fn as_str(self) -> &'static str {
        match self {
            Self::Panic => "panic",
            Self::TracingError => "tracing_error",
            Self::InternalDefect => "internal_defect",
        }
    }

    fn mechanism(self) -> &'static str {
        match self {
            Self::Panic => "panic",
            Self::TracingError => "tracing",
            Self::InternalDefect => "fission",
        }
    }

    fn level(self) -> Level {
        match self {
            Self::Panic => Level::Fatal,
            Self::TracingError | Self::InternalDefect => Level::Error,
        }
    }

    fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.as_str() == text)
    }
}

/// The DSN to report to, or `None` when reporting is not enabled.
fn dsn_from(lookup: impl Fn(&str) -> Option<String>) -> Option<String> {
    if lookup("FISSION_SENTRY").as_deref() != Some("1") {
        return None;
    }
    lookup("SENTRY_DSN")
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
}

/// Start reporting if the environment asks for it. Call once, from a binary's
/// `main`, before logging is initialised (the tracing hook is read then).
///
/// `fission_cli` calls this only in the process that does the work: under
/// `--contract v1` that is the re-executed child, never the supervisor.
pub fn init() -> Option<Guard> {
    let dsn = dsn_from(|key| std::env::var(key).ok())?;
    if dsn.parse::<sentry::types::Dsn>().is_err() {
        eprintln!("telemetry: SENTRY_DSN is not a valid DSN; reporting stays off");
        return None;
    }
    let guard = sentry::init(client_options(&dsn, |key| std::env::var(key).ok()));
    fission_core::logging::set_error_callsite_hook(report_error_callsite);
    Some(guard)
}

/// Client options: no default integrations (no contexts: they would collect
/// OS, device and host details only for [`scrub`] to drop; no breadcrumbs, no
/// stack attachment for messages), our own panic extractor, and the scrub
/// plus per-process de-duplication as `before_send`.
fn client_options(dsn: &str, lookup: impl Fn(&str) -> Option<String>) -> sentry::ClientOptions {
    let reported = Mutex::new(BTreeSet::new());
    sentry::ClientOptions::new()
        .dsn(dsn)
        .release(release(&lookup))
        .environment(environment(&lookup))
        .send_default_pii(false)
        .sample_rate(1.0)
        .traces_sample_rate(0.0)
        .attach_stacktrace(false)
        .max_breadcrumbs(0)
        .default_integrations(false)
        .add_integration(
            sentry::integrations::panic::PanicIntegration::new()
                .add_extractor(|info| Some(panic_event(info))),
        )
        .before_send(move |event| {
            let event = scrub(event);
            let mut reported = reported.lock().unwrap_or_else(|e| e.into_inner());
            admit(&mut reported, &event).then_some(event)
        })
}

/// Release: SENTRY_RELEASE, then FISSION_RELEASE, then `fission-cli@<version>`.
/// A value [`clean_release`] rejects falls through to the next.
fn release(lookup: impl Fn(&str) -> Option<String>) -> Cow<'static, str> {
    ["SENTRY_RELEASE", "FISSION_RELEASE"]
        .iter()
        .filter_map(|key| lookup(key))
        .find_map(|v| clean_release(v.trim()).map(str::to_owned))
        .map(Cow::Owned)
        .unwrap_or_else(|| sentry::release_name!().unwrap_or(Cow::Borrowed("fission-cli@unknown")))
}

/// Environment: SENTRY_ENVIRONMENT when it is one of [`ENVIRONMENTS`], else `production`.
fn environment(lookup: impl Fn(&str) -> Option<String>) -> Cow<'static, str> {
    Cow::Borrowed(
        lookup("SENTRY_ENVIRONMENT")
            .as_deref()
            .map(str::trim)
            .and_then(|e| one_of(e, ENVIRONMENTS))
            .unwrap_or("production"),
    )
}

/// Send queued events before the process exits. `process::exit` skips guard drops.
pub fn flush() {
    if let Some(client) = sentry::Hub::current().client() {
        client.flush(Some(Duration::from_secs(2)));
    }
}

/// Report an internal defect at the caller's location. There is deliberately
/// no message argument: the call site identifies the defect. Expected
/// analysis failures are not defects and must not be reported.
#[track_caller]
pub fn capture_defect() {
    if sentry::Hub::current().client().is_some() {
        let location = std::panic::Location::caller();
        sentry::capture_event(callsite_event(
            ReportKind::InternalDefect,
            Some((location.file(), location.line())),
            None,
        ));
    }
}

/// Panic extractor: reads `info.location()` and never `info.payload()`.
fn panic_event(info: &PanicHookInfo<'_>) -> Event<'static> {
    let location = info.location().map(|l| (l.file(), l.line()));
    let stacktrace = sentry::integrations::backtrace::current_stacktrace();
    callsite_event(ReportKind::Panic, location, stacktrace)
}

/// [`fission_core::logging::ErrorCallsiteHook`]: an ERROR tracing event at a
/// callsite. Only static metadata arrives here.
fn report_error_callsite(metadata: &'static tracing::Metadata<'static>) {
    let location = metadata.file().zip(metadata.line());
    sentry::capture_event(callsite_event(ReportKind::TracingError, location, None));
}

/// The single constructor for outgoing events. The callsite travels in the
/// `location` tag as `path:line`; [`scrub`] re-validates it.
fn callsite_event(
    kind: ReportKind,
    location: Option<(&str, u32)>,
    stacktrace: Option<Stacktrace>,
) -> Event<'static> {
    let mut event = Event {
        level: kind.level(),
        exception: vec![Exception {
            ty: kind.as_str().to_owned(),
            mechanism: Some(Mechanism {
                ty: kind.mechanism().to_owned(),
                handled: Some(kind != ReportKind::Panic),
                ..Default::default()
            }),
            stacktrace,
            ..Default::default()
        }]
        .into(),
        ..Default::default()
    };
    if let Some(location) = location.and_then(|(file, line)| clean_location(file, line)) {
        event.tags.insert(LOCATION_TAG.into(), location);
    }
    event
}

/// `s` when it is one of `allowed`, as the `'static` entry.
fn one_of(s: &str, allowed: &[&'static str]) -> Option<&'static str> {
    allowed.iter().copied().find(|a| *a == s)
}

/// `path:line` for a source file [`clean_source_path`] accepts.
fn clean_location(file: &str, line: u32) -> Option<String> {
    clean_source_path(file).map(|p| format!("{p}:{line}"))
}

/// A source path as one of three public forms, or `None`:
///
/// * Fission's own source, workspace-relative (`crates/fission-x/src/a.rs`);
///   rustc records workspace members relative, and an absolute path (a git
///   checkout, `cargo install --git`) is reduced to the part from
///   `crates/fission-` on;
/// * the standard library (`/rustc/<commit>/library/...` -> `library/...`);
/// * a crates.io dependency (`.../registry/src/<index>/<crate>-<ver>/...` ->
///   `deps/<crate>-<ver>/...`).
///
/// Anything else (a home directory, a corpus file, a Windows drive or UNC
/// path outside those forms) is dropped, and so is a result with any byte
/// outside `[a-z0-9_./-]`: every Fission source path is lowercase.
fn clean_source_path(raw: &str) -> Option<String> {
    let p = raw.replace('\\', "/");
    let candidate = if let Some(rest) = p.strip_prefix("/rustc/") {
        rest.split_once('/')?.1.to_owned()
    } else if let Some((_, rest)) = p.split_once("/registry/src/") {
        format!("deps/{}", rest.split_once('/')?.1)
    } else if p.starts_with(OWN_CRATES) {
        p
    } else {
        let i = p.find(&format!("/{OWN_CRATES}"))?;
        p[i + 1..].to_owned()
    };
    is_clean_source_path(&candidate).then_some(candidate)
}

fn is_clean_source_path(p: &str) -> bool {
    let mut parts = p.split('/');
    let root = parts.next().unwrap_or_default();
    p.len() <= 200
        && p.ends_with(".rs")
        && SOURCE_ROOTS.contains(&root)
        && (root != "crates" || p.starts_with(OWN_CRATES))
        && parts.all(|c| !c.is_empty() && c != "." && c != "..")
        && p.bytes().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'_' | b'.' | b'/' | b'-')
        })
}

fn is_clean_location(location: &str) -> bool {
    location.rsplit_once(':').is_some_and(|(path, line)| {
        is_clean_source_path(path)
            && !line.is_empty()
            && line.len() <= 10
            && line.bytes().all(|b| b.is_ascii_digit())
    })
}

/// A release name `fission-cli@<version>[+build]`, or `None`.
fn clean_release(s: &str) -> Option<&str> {
    let version = s.strip_prefix(RELEASE_PREFIX)?;
    (!version.is_empty()
        && version.len() <= 100
        && version
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'-' | b'+')))
    .then_some(s)
}

/// A version such as the SDK's `0.49.6`, or `None`.
fn clean_version(s: &str) -> Option<&str> {
    (!s.is_empty()
        && s.len() <= 32
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'+')))
    .then_some(s)
}

/// The crate a demangled function name starts with: `<fission_x::X as
/// core::ops::Fn>::call` -> `fission_x`.
fn root_crate(name: &str) -> &str {
    let mut s = name.trim_start_matches(['<', '&', '*', ' ', '(']);
    for prefix in ["mut ", "const ", "dyn "] {
        s = s.strip_prefix(prefix).unwrap_or(s);
    }
    let end = s
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .unwrap_or(s.len());
    &s[..end]
}

/// Crate names are lowercase; Fission's start with `fission_`.
fn is_frame_crate(root: &str) -> bool {
    let lowercase = root
        .bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_');
    lowercase && (FRAME_CRATES.contains(&root) || root.starts_with("fission_"))
}

/// A frame's function name (hash stripped) when it looks like a demangled
/// Rust path in a crate of [`FRAME_CRATES`] or a `fission_*` crate;
/// [`OTHER_FRAME`] otherwise (JIT code, foreign code, other crates,
/// unsymbolized addresses).
fn clean_function(name: &str) -> String {
    let name = match name.rsplit_once("::h") {
        Some((head, hash)) if hash.len() == 16 && hash.bytes().all(|b| b.is_ascii_hexdigit()) => {
            head
        }
        _ => name,
    };
    let rust_path = name.len() <= 512
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b" _:<>{}()[]&*,;'#!-.".contains(&b));
    let root = root_crate(name);
    let rooted = name
        .find(root)
        .is_some_and(|i| name[i + root.len()..].starts_with("::"));
    if rust_path && rooted && is_frame_crate(root) {
        name.to_owned()
    } else {
        OTHER_FRAME.to_owned()
    }
}

/// A frame keeps a cleaned function, a public source path, line and
/// `in_app`; it never keeps addresses, symbols, module/package names,
/// absolute paths, source context or locals.
fn scrub_frame(frame: Frame) -> Frame {
    let file = frame
        .abs_path
        .as_deref()
        .or(frame.filename.as_deref())
        .and_then(clean_source_path);
    let function = frame.function.as_deref().map(clean_function);
    Frame {
        in_app: Some(
            function
                .as_deref()
                .is_some_and(|f| f.starts_with("fission_") || f.contains("<fission_")),
        ),
        function,
        lineno: file.as_ref().and(frame.lineno),
        filename: file,
        ..Default::default()
    }
}

fn scrub_stacktrace(stacktrace: Stacktrace) -> Stacktrace {
    let mut frames: Vec<Frame> = stacktrace.frames.into_iter().map(scrub_frame).collect();
    // Sentry orders frames outermost first; keep the innermost ones.
    if frames.len() > MAX_FRAMES {
        frames.drain(..frames.len() - MAX_FRAMES);
    }
    Stacktrace {
        frames,
        ..Default::default()
    }
}

/// Last line of defence before an event leaves the process. Allowlist: the
/// event is rebuilt from the fields named in the module documentation, each
/// checked against its own rule; nothing else is copied (exception
/// module, extra, other tags, contexts, breadcrumbs, user, request, server
/// name, modules, threads, debug images, culprit, transaction, logger,
/// template, dist). Pure: the output depends only on the input.
pub(crate) fn scrub(event: Event<'static>) -> Event<'static> {
    let location = event
        .tags
        .get(LOCATION_TAG)
        .filter(|l| is_clean_location(l))
        .cloned();
    let source = event.exception.values.into_iter().next();
    let kind = source.as_ref().and_then(|e| ReportKind::parse(&e.ty));
    let ty = kind.map_or(UNKNOWN_TYPE, ReportKind::as_str);

    let mut out = Event::new();
    out.event_id = event.event_id;
    out.timestamp = event.timestamp;
    out.level = kind.map_or(Level::Error, ReportKind::level);
    out.platform = Cow::Borrowed(one_of(&event.platform, PLATFORMS).unwrap_or("native"));
    out.release = event
        .release
        .as_deref()
        .and_then(clean_release)
        .map(|r| Cow::Owned(r.to_owned()));
    out.environment = event
        .environment
        .as_deref()
        .and_then(|e| one_of(e, ENVIRONMENTS))
        .map(Cow::Borrowed);
    out.sdk = event.sdk.map(|sdk| {
        Cow::Owned(ClientSdkInfo {
            name: one_of(&sdk.name, SDK_NAMES).unwrap_or("unknown").to_owned(),
            version: clean_version(&sdk.version).unwrap_or("unknown").to_owned(),
            integrations: Vec::new(),
            packages: Vec::new(),
        })
    });
    out.message = event.message.map(|_| REDACTED.to_owned());
    out.logentry = event.logentry.map(|_| LogEntry {
        message: REDACTED.to_owned(),
        params: Vec::new(),
    });
    out.fingerprint = Cow::Owned(vec![
        Cow::Borrowed(ty),
        Cow::Owned(location.clone().unwrap_or_else(|| NO_LOCATION.to_owned())),
    ]);
    let stacktrace = source
        .as_ref()
        .and_then(|e| e.stacktrace.clone())
        .or(event.stacktrace)
        .map(scrub_stacktrace)
        .filter(|s| !s.frames.is_empty());
    out.exception = Values::from(vec![Exception {
        ty: ty.to_owned(),
        value: Some(match &location {
            Some(l) => format!("{ty} at {l}"),
            None => format!("{ty} (message not sent)"),
        }),
        mechanism: kind.map(|kind| Mechanism {
            ty: kind.mechanism().to_owned(),
            handled: source
                .as_ref()
                .and_then(|e| e.mechanism.as_ref())
                .and_then(|m| m.handled),
            ..Default::default()
        }),
        stacktrace,
        ..Default::default()
    }]);
    if let Some(l) = location {
        out.tags.insert(LOCATION_TAG.into(), l);
    }
    out
}

/// Per-process de-duplication by fingerprint, capped at
/// [`MAX_DISTINCT_REPORTS`] groups. Panics caught per function (`decomp
/// --all`) and errors logged in loops would otherwise send one event per
/// occurrence. Events of no known [`ReportKind`] (anything but our three
/// producers, such as a stray `sentry::capture_message`) are dropped too.
fn admit(reported: &mut BTreeSet<String>, event: &Event<'_>) -> bool {
    let known = event
        .exception
        .values
        .first()
        .is_some_and(|e| ReportKind::parse(&e.ty).is_some());
    if !known {
        return false;
    }
    let key = event.fingerprint.join("\u{1f}");
    if reported.contains(&key) || reported.len() >= MAX_DISTINCT_REPORTS {
        return false;
    }
    reported.insert(key)
}

/// Collects envelopes in memory, for tests of the whole client pipeline.
#[cfg(test)]
#[derive(Default)]
struct CollectingTransport(Mutex<Vec<Vec<u8>>>);

#[cfg(test)]
impl sentry::Transport for CollectingTransport {
    fn send_envelope(&self, envelope: sentry::Envelope) {
        let mut bytes = Vec::new();
        envelope.to_writer(&mut bytes).expect("envelope serializes");
        self.0.lock().unwrap().push(bytes);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sentry::protocol::{Breadcrumb, Context, LogEntry, Map, Request, User};
    use std::collections::BTreeMap;
    use std::sync::Arc;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn env<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |key| {
            pairs
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| (*v).to_owned())
        }
    }

    #[test]
    fn off_without_explicit_opt_in() {
        assert_eq!(
            dsn_from(env(&[("SENTRY_DSN", "https://k@o1.ingest.sentry.io/2")])),
            None
        );
        assert_eq!(
            dsn_from(env(&[
                ("FISSION_SENTRY", "0"),
                ("SENTRY_DSN", "https://k@o1/2")
            ])),
            None
        );
        assert_eq!(dsn_from(env(&[("FISSION_SENTRY", "1")])), None);
    }

    #[test]
    fn on_only_with_opt_in_and_dsn() {
        let dsn = "https://k@o1.ingest.sentry.io/2";
        let got = dsn_from(env(&[("FISSION_SENTRY", "1"), ("SENTRY_DSN", dsn)]));
        assert_eq!(got.as_deref(), Some(dsn));
        assert_eq!(
            dsn_from(env(&[("FISSION_SENTRY", "1"), ("SENTRY_DSN", "  ")])),
            None
        );
    }

    #[test]
    fn source_paths_keep_only_public_forms() {
        let cases = [
            (
                "crates/fission-pcode/src/midend/x.rs",
                Some("crates/fission-pcode/src/midend/x.rs"),
            ),
            (
                "crates\\fission-pcode\\src\\midend\\x.rs",
                Some("crates/fission-pcode/src/midend/x.rs"),
            ),
            (
                "/Users/alice/.cargo/git/checkouts/fission-1a2b/3c4d/crates/fission-cli/src/main.rs",
                Some("crates/fission-cli/src/main.rs"),
            ),
            (
                "/rustc/0123abcd/library/core/src/panicking.rs",
                Some("library/core/src/panicking.rs"),
            ),
            (
                "/Users/alice/.cargo/registry/src/index.crates.io-1949/rayon-core-1.12.1/src/job.rs",
                Some("deps/rayon-core-1.12.1/src/job.rs"),
            ),
            (
                "crates/fission-cli/tests/a.rs",
                Some("crates/fission-cli/tests/a.rs"),
            ),
            // User paths, corpus files, other workspaces and non-Rust files.
            ("/Users/alice/My Projects/src/secret plan.rs", None),
            ("/Users/alice/My Projects/secret plan.bin", None),
            ("/Users/alice/work/crates/other/src/a.rs", None),
            ("crates/other/src/a.rs", None),
            ("C:\\Users\\alice\\fission-x\\src\\a b.rs", None),
            ("\\\\srv\\share\\crates\\fission-x\\src\\a.bin", None),
            ("crates/fission-x/src/../../../etc/passwd.rs", None),
            ("crates/fission-x/src/Decrypt.rs", None),
            ("main.rs", None),
            ("", None),
        ];
        for (input, want) in cases {
            assert_eq!(clean_source_path(input).as_deref(), want, "{input}");
        }
    }

    #[test]
    fn locations_need_a_numeric_line() {
        assert!(is_clean_location("crates/fission-x/src/a.rs:42"));
        assert!(!is_clean_location("crates/fission-x/src/a.rs"));
        assert!(!is_clean_location("crates/fission-x/src/a.rs:4x"));
        assert!(!is_clean_location("decrypt_license_key:1"));
        assert_eq!(
            clean_location("/rustc/ab/library/std/src/a.rs", 3).as_deref(),
            Some("library/std/src/a.rs:3")
        );
    }

    #[test]
    fn frame_functions_keep_only_known_crates() {
        let cases = [
            (
                "fission_pcode::midend::run::h0123456789abcdef",
                "fission_pcode::midend::run",
            ),
            (
                "<fission_x::Foo as core::ops::Drop>::drop",
                "<fission_x::Foo as core::ops::Drop>::drop",
            ),
            (
                "std::panicking::begin_panic::{{closure}}",
                "std::panicking::begin_panic::{{closure}}",
            ),
            ("rayon_core::registry::run", "rayon_core::registry::run"),
            ("hashbrown::raw::find", OTHER_FRAME),
            ("main", OTHER_FRAME),
            ("<unknown>", OTHER_FRAME),
            ("decrypt_license_key", OTHER_FRAME),
            ("sub_401000", OTHER_FRAME),
            ("fission_x", OTHER_FRAME),
            ("fission_Secret::x", OTHER_FRAME),
            ("fission_x::a b\"c", OTHER_FRAME),
        ];
        for (input, want) in cases {
            assert_eq!(clean_function(input), want, "{input}");
        }
    }

    #[test]
    fn release_and_environment_are_clean_or_default() {
        let release_of = |pairs: &[(&str, &str)]| release(env(pairs)).into_owned();
        assert_eq!(
            release_of(&[("SENTRY_RELEASE", "fission-cli@abc123")]),
            "fission-cli@abc123"
        );
        assert_eq!(
            release_of(&[
                ("SENTRY_RELEASE", "/Users/alice/secret"),
                ("FISSION_RELEASE", "fission-cli@1.2.3+g1")
            ]),
            "fission-cli@1.2.3+g1"
        );
        assert!(release_of(&[("SENTRY_RELEASE", "token=ghp_x")]).starts_with("fission-cli@"));
        assert_eq!(environment(env(&[("SENTRY_ENVIRONMENT", "ci")])), "ci");
        assert_eq!(
            environment(env(&[("SENTRY_ENVIRONMENT", "alice-laptop")])),
            "production"
        );
    }

    #[test]
    fn unknown_exception_types_are_not_forwarded() {
        let mut event = Event::new();
        event.exception.values.push(Exception {
            ty: "decrypt_license_key".into(),
            value: Some("decrypt_license_key".into()),
            module: Some("decrypt_license_key".into()),
            ..Default::default()
        });
        let out = scrub(event);
        assert_eq!(out.exception.values[0].ty, UNKNOWN_TYPE);
        let json = serde_json::to_string(&out).unwrap();
        assert!(!json.contains("decrypt_license_key"), "{json}");
        assert!(!admit(&mut BTreeSet::new(), &out));
    }

    #[test]
    fn fingerprint_is_kind_and_location() {
        let event = scrub(callsite_event(
            ReportKind::Panic,
            Some(("crates/fission-pcode/src/a.rs", 7)),
            None,
        ));
        assert_eq!(
            event.fingerprint.as_ref(),
            ["panic", "crates/fission-pcode/src/a.rs:7"]
        );
        assert_eq!(
            event.tags.get(LOCATION_TAG).map(String::as_str),
            Some("crates/fission-pcode/src/a.rs:7")
        );
        assert_eq!(event.tags.len(), 1);
        assert_eq!(event.level, Level::Fatal);
        let exception = &event.exception.values[0];
        assert_eq!(
            exception.value.as_deref(),
            Some("panic at crates/fission-pcode/src/a.rs:7")
        );
        assert_eq!(exception.mechanism.as_ref().unwrap().handled, Some(false));

        let other = scrub(callsite_event(
            ReportKind::TracingError,
            Some(("crates/fission-pcode/src/a.rs", 7)),
            None,
        ));
        assert_ne!(event.fingerprint, other.fingerprint);
        assert_eq!(other.level, Level::Error);

        let nowhere = scrub(callsite_event(ReportKind::Panic, None, None));
        assert_eq!(nowhere.fingerprint.as_ref(), ["panic", NO_LOCATION]);
        assert_eq!(
            nowhere.exception.values[0].value.as_deref(),
            Some("panic (message not sent)")
        );
        assert!(nowhere.tags.is_empty());
    }

    #[test]
    fn scrub_is_idempotent() {
        let once = scrub(callsite_event(
            ReportKind::TracingError,
            Some(("crates/fission-x/src/a.rs", 3)),
            Some(Stacktrace {
                frames: vec![trusted_frame()],
                ..Default::default()
            }),
        ));
        let twice = scrub(once.clone());
        assert_eq!(
            serde_json::to_value(&once).unwrap(),
            serde_json::to_value(&twice).unwrap()
        );
    }

    #[test]
    fn duplicates_and_overflow_are_dropped() {
        let mut reported = BTreeSet::new();
        let a = scrub(callsite_event(
            ReportKind::Panic,
            Some(("crates/fission-x/src/a.rs", 1)),
            None,
        ));
        assert!(admit(&mut reported, &a));
        assert!(!admit(&mut reported, &a));
        for line in 2..=(MAX_DISTINCT_REPORTS as u32 + 8) {
            let e = scrub(callsite_event(
                ReportKind::Panic,
                Some(("crates/fission-x/src/a.rs", line)),
                None,
            ));
            admit(&mut reported, &e);
        }
        assert_eq!(reported.len(), MAX_DISTINCT_REPORTS);
    }

    fn trusted_frame() -> Frame {
        Frame {
            function: Some("fission_pcode::midend::run::h0123456789abcdef".into()),
            abs_path: Some("/home/user/fission/crates/fission-pcode/src/midend/run.rs".into()),
            filename: Some("run.rs".into()),
            lineno: Some(12),
            instruction_addr: Some(sentry::protocol::Addr(0x40_1000)),
            ..Default::default()
        }
    }

    /// Plant `data` in every free-text field an event can carry, including
    /// the ones the PR's first scrub copied through (frame function/symbol/
    /// module/package/abs_path, `modules`, exception module, logger).
    fn event_with_data(data: &[String]) -> Event<'static> {
        let pick = |i: usize| data[i % data.len()].clone();
        let mut event = Event::new();
        event.event_id = sentry::types::Uuid::nil();
        event.timestamp = UNIX_EPOCH + Duration::from_secs(1);
        event.release = Some("fission-cli@0.0.0".into());
        event.environment = Some("test".into());
        event.message = Some(pick(0));
        event.logentry = Some(LogEntry {
            message: pick(1),
            params: vec![pick(2).into()],
        });
        event.logger = Some(pick(3));
        event.culprit = Some(pick(4));
        event.dist = Some(pick(5).into());
        event.server_name = Some(pick(6).into());
        event.modules = BTreeMap::from([(pick(7), pick(8))]);
        event.tags.insert(pick(9), pick(10));
        event.extra.insert(pick(11), pick(12).into());
        event.contexts.insert(
            pick(13),
            Context::Other(Map::from_iter([(pick(14), pick(15).into())])),
        );
        event.breadcrumbs.values.push(Breadcrumb {
            message: Some(pick(16)),
            ..Default::default()
        });
        event.user = Some(User {
            username: Some(pick(17)),
            ..Default::default()
        });
        event.request = Some(Request {
            query_string: Some(pick(18)),
            ..Default::default()
        });
        let data_frame = |i: usize| Frame {
            function: Some(pick(i)),
            symbol: Some(pick(i + 1)),
            module: Some(pick(i + 2)),
            package: Some(pick(i + 3)),
            abs_path: Some(pick(i + 4)),
            filename: Some(pick(i + 5)),
            context_line: Some(pick(i + 6)),
            pre_context: vec![pick(i + 7)],
            post_context: vec![pick(i + 8)],
            vars: BTreeMap::from([(pick(i + 9), pick(i + 10).into())]),
            lineno: Some(3),
            instruction_addr: Some(sentry::protocol::Addr(0x40_1000)),
            ..Default::default()
        };
        // A trusted Fission frame whose metadata fields carry data.
        let mut mixed = trusted_frame();
        mixed.module = Some(pick(30));
        mixed.package = Some(pick(31));
        mixed.symbol = Some(pick(32));
        mixed.context_line = Some(pick(33));
        mixed.filename = Some(pick(34));
        event.exception.values.push(Exception {
            ty: pick(19),
            value: Some(pick(20)),
            module: Some(pick(21)),
            mechanism: Some(Mechanism {
                ty: pick(22),
                description: Some(pick(23)),
                data: Map::from_iter([(pick(24), pick(25).into())]),
                ..Default::default()
            }),
            stacktrace: Some(Stacktrace {
                frames: vec![data_frame(40), mixed, data_frame(60)],
                ..Default::default()
            }),
            ..Default::default()
        });
        event.stacktrace = Some(Stacktrace {
            frames: vec![data_frame(80)],
            ..Default::default()
        });
        event.transaction = Some(format!("{}:12", pick(26)));
        event
            .tags
            .insert(LOCATION_TAG.into(), format!("{}:12", pick(27)));
        event
    }

    /// Deterministic xorshift64* generator.
    struct Rng(u64);

    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 >> 12;
            self.0 ^= self.0 << 25;
            self.0 ^= self.0 >> 27;
            self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
        }

        fn pick<'a>(&mut self, items: &[&'a str]) -> &'a str {
            items[(self.next() % items.len() as u64) as usize]
        }
    }

    /// Marker letters never emitted by a scrubbed event built from the
    /// trusted parts above (lowercase paths/symbols, nil UUID, fixed time),
    /// so any 4-byte window containing one is analyzed data.
    const MARKERS: &[u8] = b"JKQVWXZ";

    /// "Analyzed data": tokens an attacker controls in a binary (strings,
    /// symbol names, paths, credentials), embedded in shapes that real
    /// panic/tracing messages take.
    fn analyzed_data(rng: &mut Rng) -> String {
        let mut token = String::new();
        let len = 4 + rng.next() % 12;
        for _ in 0..len {
            let b = match rng.next() % 4 {
                0 => MARKERS[(rng.next() % MARKERS.len() as u64) as usize],
                1 => b'a' + (rng.next() % 26) as u8,
                2 => b'0' + (rng.next() % 10) as u8,
                _ => b'_',
            };
            token.push(b as char);
        }
        // Every token carries a marker so its windows are checkable.
        token.push(MARKERS[(rng.next() % MARKERS.len() as u64) as usize] as char);
        let shape = rng.pick(&[
            "{t}",
            "unexpected expr Call(\"{t}\") in sub_{t}",
            "token={t}",
            "Authorization: Bearer {t}",
            "https://{t}:{t}@example.invalid/x",
            "https://{t}@o1.ingest.sentry.io/{t}",
            "/Users/{t}/My Projects/{t} plan.bin",
            "C:\\Users\\{t}\\{t}.exe",
            "\\\\{t}\\share\\{t}.bin",
            "fission_{t}::{t}",
            "crates/fission-{t}/src/{t}.rs",
            "/Users/{t}/.cargo/registry/src/index/{t}-1.0/src/lib.rs",
            "/rustc/{t}/library/{t}/src/lib.rs",
            "{t}::{t}",
            "<{t} as fission_x::Foo>::bar",
        ]);
        shape.replace("{t}", &token)
    }

    fn assert_no_data_window(json: &str, data: &[String]) {
        for item in data {
            let bytes = item.as_bytes();
            for window in bytes.windows(4) {
                if !window.iter().any(|b| MARKERS.contains(b)) {
                    continue;
                }
                let window = std::str::from_utf8(window).unwrap();
                assert!(
                    !json.contains(window),
                    "analyzed-data window {window:?} from {item:?} leaked into {json}"
                );
            }
        }
    }

    #[test]
    fn no_analyzed_data_window_survives_scrub() {
        let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
        for _ in 0..500 {
            let data: Vec<String> = (0..96).map(|_| analyzed_data(&mut rng)).collect();
            let json = serde_json::to_string(&scrub(event_with_data(&data))).unwrap();
            assert!(
                !json.bytes().any(|b| MARKERS.contains(&b)),
                "a marker byte leaked: {json}"
            );
            assert_no_data_window(&json, &data);
        }
    }

    #[test]
    fn known_leaks_from_the_first_scrub_are_closed() {
        let probes = [
            "token=ghp_AbCdEf123456",
            "Authorization: Bearer eyJhbGciOiJIUzI1NiJ9.payload.sig",
            "/Users/alice/My Projects/secret plan.bin",
            "unexpected expr Call(\"decrypt_license_key\") in sub_401000",
            "https://user:hunter2@example.com/x",
            "https://abcdef0123@o1.ingest.sentry.io/42",
            "C:\\Users\\alice\\secret\\b.exe",
            "\\\\srv\\share\\c.bin",
        ];
        let data: Vec<String> = probes.iter().map(|p| (*p).to_owned()).collect();
        let json = serde_json::to_string(&scrub(event_with_data(&data))).unwrap();
        for needle in [
            "ghp_",
            "AbCdEf",
            "Bearer",
            "eyJ",
            "alice",
            "Projects",
            "secret",
            "plan",
            "decrypt",
            "license",
            "sub_4",
            "hunter2",
            "user:",
            "abcdef0123",
            "srv",
            "share",
            "b.exe",
            "c.bin",
            "401000",
            "4198400",
        ] {
            assert!(!json.contains(needle), "{needle:?} leaked into {json}");
        }
        // What does survive is the trusted Fission frame.
        assert!(json.contains("fission_pcode::midend::run"), "{json}");
        assert!(
            json.contains("crates/fission-pcode/src/midend/run.rs"),
            "{json}"
        );
    }

    /// The whole client path, through a real `sentry::Client` and an
    /// in-memory transport: a panic whose payload and a tracing error whose
    /// message carry analyzed data, then the bytes that would go on the wire.
    #[test]
    fn wire_envelopes_carry_no_panic_payload() {
        let transport = Arc::new(CollectingTransport::default());
        let mut options = client_options("https://public@o0.ingest.invalid/0", |_| None);
        options.transport = Some(Arc::new(transport.clone()));
        options.release = Some("fission-cli@0.0.0".into());
        let client = Arc::new(sentry::Client::with_options(options));
        let hub = Arc::new(sentry::Hub::new(
            Some(client.clone()),
            Arc::new(sentry::Scope::default()),
        ));
        let secret = "decrypt_license_key_QZX";
        sentry::Hub::run(hub, || {
            for _ in 0..3 {
                let result = std::panic::catch_unwind(|| {
                    panic!("unexpected expr Call(\"{secret}\") in sub_401000 /Users/alice/x y.bin")
                });
                assert!(result.is_err());
            }
            capture_defect();
            sentry::capture_message(secret, Level::Error);
        });
        client.flush(Some(Duration::from_secs(5)));
        let envelopes = transport.0.lock().unwrap().clone();
        // Three identical panics de-duplicate to one and the defect is one
        // more; the free-text message has no report kind and is dropped.
        assert_eq!(envelopes.len(), 2, "{envelopes:?}");
        let wire = envelopes
            .iter()
            .map(|e| String::from_utf8_lossy(e).into_owned())
            .collect::<Vec<_>>()
            .join("\n");
        for needle in [secret, "decrypt", "sub_401000", "alice", "x y.bin", "QZX"] {
            assert!(!wire.contains(needle), "{needle:?} leaked into {wire}");
        }
        assert!(wire.contains("\"type\":\"panic\""), "{wire}");
        assert!(
            wire.contains("\"location\":\"crates/fission-cli/src/telemetry.rs:"),
            "{wire}"
        );
        assert!(wire.contains("\"type\":\"internal_defect\""), "{wire}");
    }

    #[test]
    fn panic_fingerprint_comes_from_the_hook_location() {
        let event = scrub(callsite_event(
            ReportKind::Panic,
            Some((file!(), line!())),
            None,
        ));
        let location = &event.fingerprint[1];
        assert!(
            location.starts_with("crates/fission-cli/src/telemetry.rs:"),
            "{location}"
        );
        assert!(SystemTime::now() > event.timestamp);
    }
}
