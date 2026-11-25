//! Opt-in error reporting to Sentry, compiled only with the `sentry` feature.
//!
//! Nothing is sent unless `FISSION_SENTRY=1` and `SENTRY_DSN` are both set.
//! Panics and `tracing` ERROR events are reported. Binaries, decompiled
//! output, memory, environment variables and unreduced paths never leave the
//! process: an allowlist scrub rebuilds the event before it is sent.

use sentry::protocol::{Event, Exception, Frame, Stacktrace, Values};
use std::borrow::Cow;
use std::time::Duration;

/// Keeps the Sentry client alive; dropping it flushes queued events.
pub type Guard = sentry::ClientInitGuard;

/// The DSN to report to, or `None` when reporting is not enabled.
fn dsn_from(lookup: impl Fn(&str) -> Option<String>) -> Option<String> {
    if lookup("FISSION_SENTRY").as_deref() != Some("1") {
        return None;
    }
    lookup("SENTRY_DSN")
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
}

/// Start reporting if the environment asks for it. Call once, from a binary's `main`.
pub fn init() -> Option<Guard> {
    let dsn = dsn_from(|key| std::env::var(key).ok())?;
    if dsn.parse::<sentry::types::Dsn>().is_err() {
        eprintln!("telemetry: SENTRY_DSN is not a valid DSN; reporting stays off");
        return None;
    }
    Some(sentry::init(
        sentry::ClientOptions::new()
            .dsn(&dsn)
            .release(release())
            .send_default_pii(false)
            .sample_rate(1.0)
            .traces_sample_rate(0.0)
            .attach_stacktrace(true)
            .max_breadcrumbs(0)
            .before_send(|event| Some(scrub(event))),
    ))
}

/// Release identifier: SENTRY_RELEASE, then FISSION_RELEASE, then Cargo package release.
fn release() -> Cow<'static, str> {
    if let Ok(r) = std::env::var("SENTRY_RELEASE") {
        let r = r.trim().to_owned();
        if !r.is_empty() {
            return Cow::Owned(r);
        }
    }
    if let Ok(r) = std::env::var("FISSION_RELEASE") {
        let r = r.trim().to_owned();
        if !r.is_empty() {
            return Cow::Owned(r);
        }
    }
    sentry::release_name!().unwrap_or_else(|| Cow::Borrowed("fission-cli@unknown"))
}

/// Send queued events before the process exits. `process::exit` skips guard drops.
pub fn flush() {
    if let Some(client) = sentry::Hub::current().client() {
        client.flush(Some(Duration::from_secs(2)));
    }
}

/// Capture an internal error explicitly. Expected analysis failures should not
/// be reported; call this only for internal defects or infrastructure failures.
pub fn capture_error(err: &(dyn std::error::Error + 'static)) {
    if sentry::Hub::current().client().is_some() {
        sentry::capture_error(err);
    }
}

fn home_dir() -> Option<String> {
    std::env::var("HOME")
        .ok()
        .or_else(|| std::env::var("USERPROFILE").ok())
        .filter(|h| h.len() > 1)
}

fn is_secret_token(word: &str) -> bool {
    let w =
        word.trim_matches(|c: char| matches!(c, '"' | '\'' | ',' | ';' | ')' | ']' | '}' | '>'));
    let lower = w.to_ascii_lowercase();
    lower.starts_with("sntrys_")
        || lower.starts_with("sntryu_")
        || lower.starts_with("ghp_")
        || lower.starts_with("gho_")
        || lower.starts_with("ghu_")
        || lower.starts_with("ghs_")
        || lower.starts_with("github_pat_")
        || lower.starts_with("sk-")
        || lower.starts_with("xoxb-")
        || lower.starts_with("xoxp-")
}

fn looks_like_hex_blob(word: &str) -> bool {
    let bytes = word.as_bytes();
    bytes.len() >= 64 && bytes.iter().all(|b| b.is_ascii_hexdigit())
}

fn looks_like_path_token(word: &str) -> bool {
    if word.contains('/') && word.len() > 1 {
        return true;
    }
    if word.contains('\\') && word.len() > 1 {
        return true;
    }
    let b = word.as_bytes();
    b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && (b[2] == b'\\' || b[2] == b'/')
}

/// Replace absolute paths, home dirs, and obvious secrets in free text.
fn redact_text(text: &str) -> String {
    let home = home_dir();
    text.split_whitespace()
        .map(|word| {
            if is_secret_token(word) {
                return "<redacted>".to_owned();
            }
            if looks_like_hex_blob(word) {
                return "<redacted>".to_owned();
            }
            let mut w = word.to_owned();
            if let Some(h) = &home {
                w = w.replace(h.as_str(), "~");
            }
            if looks_like_path_token(&w) {
                let base = base_name(&w);
                if base.is_empty() {
                    "<path>".to_owned()
                } else {
                    format!("<path>/{base}")
                }
            } else {
                w
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn base_name(path: &str) -> String {
    let base = path.rsplit(['/', '\\']).next().unwrap_or(path).trim();
    base.trim_end_matches(|c: char| {
        matches!(c, '"' | '\'' | ')' | ']' | '}' | '>' | ',' | ';' | ':')
    })
    .to_owned()
}

fn scrub_frame(frame: Frame) -> Frame {
    Frame {
        function: frame.function,
        symbol: frame.symbol,
        module: frame.module,
        package: frame.package,
        filename: frame.filename.map(|p| base_name(&p)),
        abs_path: frame.abs_path.map(|p| base_name(&p)),
        lineno: frame.lineno,
        colno: frame.colno,
        pre_context: Vec::new(),
        context_line: None,
        post_context: Vec::new(),
        in_app: frame.in_app,
        instruction_addr: frame.instruction_addr,
        addr_mode: frame.addr_mode,
        vars: Default::default(),
        ..Default::default()
    }
}

fn scrub_stacktrace(stacktrace: Stacktrace) -> Stacktrace {
    Stacktrace {
        frames: stacktrace.frames.into_iter().map(scrub_frame).collect(),
        frames_omitted: stacktrace.frames_omitted,
        registers: Default::default(),
        ..Default::default()
    }
}

fn scrub_exception(exception: Exception) -> Exception {
    Exception {
        ty: exception.ty,
        value: exception.value.map(|v| redact_text(&v)),
        module: exception.module,
        stacktrace: exception.stacktrace.map(scrub_stacktrace),
        raw_stacktrace: None,
        ..Default::default()
    }
}

/// Last line of defence before an event leaves the process. Allowlist policy:
/// only fields explicitly rebuilt below are kept.
fn scrub(event: Event<'static>) -> Event<'static> {
    let mut out = Event::new();
    out.event_id = event.event_id;
    out.level = event.level;
    out.timestamp = event.timestamp;
    out.fingerprint = Cow::Owned(vec![Cow::Borrowed("{{ default }}")]);
    out.platform = event.platform;
    out.release = event.release;
    out.dist = event.dist;
    out.environment = event.environment;
    out.transaction = event.transaction.map(|t| redact_text(&t));
    out.message = event.message.map(|m| redact_text(&m));
    out.logger = event.logger;
    out.modules = event.modules;
    out.extra = Default::default();
    out.tags = Default::default();
    out.breadcrumbs = Values::new();
    out.contexts = Default::default();
    out.user = None;
    out.request = None;
    out.server_name = None;
    out.sdk = event.sdk;
    out.exception = Values::from_iter(event.exception.into_iter().map(scrub_exception));
    out.stacktrace = event.stacktrace.map(scrub_stacktrace);
    out.threads = Values::new();
    // logentry may contain unsanitized formatted messages from tracing
    out.logentry = event.logentry.map(|mut l| {
        l.message = redact_text(&l.message);
        l.params = Vec::new();
        l
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use sentry::protocol::{Breadcrumb, LogEntry, Stacktrace, Values};
    use std::collections::BTreeMap;

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
    fn redacts_unix_and_windows_paths() {
        let out = redact_text(
            "cannot read /data/corpus/m1/a.c C:\\Users\\alice\\secret\\b.exe \\\\srv\\share\\c.bin",
        );
        assert!(out.contains("<path>/a.c"));
        assert!(out.contains("<path>/b.exe"));
        assert!(out.contains("<path>/c.bin"));
        assert!(!out.contains("alice"));
        assert!(!out.contains("corpus"));
    }

    #[test]
    fn redacts_tokens_and_hex_blobs() {
        let hex = "a".repeat(64);
        let out = redact_text(&format!("tok sntryu_secret ghp_abcdef {hex}"));
        assert!(!out.contains("sntryu_secret"));
        assert!(!out.contains("ghp_abcdef"));
        assert!(!out.contains(&hex));
        assert!(out.contains("<redacted>"));
    }

    #[test]
    fn scrub_allowlist_drops_sensitive_fields() {
        let mut event = Event::new();
        event.message = Some("failed in /home/alice/build/x sntryu_secret".into());
        event.transaction = Some("/home/alice/bin/fission".into());
        event.breadcrumbs = Values::new();
        event.breadcrumbs.values.push(Breadcrumb {
            message: Some("secret".into()),
            ..Default::default()
        });
        event.extra.insert("argv".into(), "--token abc".into());
        event.tags.insert("path".into(), "/home/alice/x".into());
        event.server_name = Some("alice-laptop".into());
        event.logentry = Some(LogEntry {
            message: "read /home/alice/secret.bin".into(),
            params: vec!["/home/alice/secret.bin".into()],
        });
        let mut frame = Frame {
            abs_path: Some("/home/alice/src/decomp.rs".into()),
            filename: Some("/home/alice/src/decomp.rs".into()),
            function: Some("decomp::run".into()),
            context_line: Some("let key = \"secret\";".into()),
            pre_context: vec!["a".into()],
            post_context: vec!["b".into()],
            ..Default::default()
        };
        frame.vars = BTreeMap::from([("buf".to_owned(), "binary bytes".into())]);
        event.exception.values.push(Exception {
            value: Some("panic at /home/alice/src/decomp.rs ghp_secret".into()),
            stacktrace: Some(Stacktrace {
                frames: vec![frame],
                ..Default::default()
            }),
            ..Default::default()
        });
        event.stacktrace = Some(Stacktrace {
            frames: vec![Frame {
                abs_path: Some("C:\\Users\\alice\\x.rs".into()),
                filename: Some("C:\\Users\\alice\\x.rs".into()),
                ..Default::default()
            }],
            ..Default::default()
        });

        let event = scrub(event);
        assert!(event.breadcrumbs.values.is_empty());
        assert!(event.extra.is_empty());
        assert!(event.tags.is_empty());
        assert!(event.contexts.is_empty());
        assert!(event.threads.values.is_empty());
        assert!(event.server_name.is_none());
        assert!(event.user.is_none());
        assert!(event.request.is_none());
        let msg = event.message.as_deref().unwrap_or("");
        assert!(!msg.contains("alice"));
        assert!(!msg.contains("sntryu_secret"));
        let tx = event.transaction.as_deref().unwrap_or("");
        assert!(!tx.contains("alice"));

        let log = event.logentry.as_ref().unwrap();
        assert!(!log.message.contains("alice"));
        assert!(log.params.is_empty());

        let exception = &event.exception.values[0];
        let val = exception.value.as_deref().unwrap_or("");
        assert!(!val.contains("alice"));
        assert!(!val.contains("ghp_secret"));
        let frame = &exception.stacktrace.as_ref().unwrap().frames[0];
        assert_eq!(frame.abs_path.as_deref(), Some("decomp.rs"));
        assert_eq!(frame.filename.as_deref(), Some("decomp.rs"));
        assert!(frame.vars.is_empty());
        assert!(frame.pre_context.is_empty());
        assert!(frame.post_context.is_empty());
        assert!(frame.context_line.is_none());

        let top = &event.stacktrace.as_ref().unwrap().frames[0];
        assert_eq!(top.abs_path.as_deref(), Some("x.rs"));
    }
}
