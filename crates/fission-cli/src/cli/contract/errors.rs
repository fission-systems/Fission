//! Error kinds, the stable error-code registry, and exit-code mapping.

use serde_json::{Value, json};

/// Coarse class of a failure; each kind has one exit code.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorKind {
    Failed,
    Usage,
    Input,
    Partial,
    Timeout,
    Environment,
    Internal,
}

impl ErrorKind {
    pub fn exit_code(self) -> i32 {
        match self {
            Self::Failed => 1,
            Self::Usage => 2,
            Self::Input => 3,
            Self::Partial => 4,
            Self::Timeout => 5,
            Self::Environment => 6,
            Self::Internal => 70,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Failed => "failed",
            Self::Usage => "usage",
            Self::Input => "input",
            Self::Partial => "partial",
            Self::Timeout => "timeout",
            Self::Environment => "environment",
            Self::Internal => "internal",
        }
    }
}

/// One entry of the append-only error-code registry.
pub struct CodeInfo {
    pub code: &'static str,
    pub kind: ErrorKind,
    pub retryable: bool,
    pub summary: &'static str,
}

/// Stable error codes. Codes are never renamed or reused; add new ones.
pub const ERROR_CODES: &[CodeInfo] = &[
    CodeInfo {
        code: "usage_error",
        kind: ErrorKind::Usage,
        retryable: false,
        summary: "Invalid arguments or option combination.",
    },
    CodeInfo {
        code: "json_required",
        kind: ErrorKind::Usage,
        retryable: false,
        summary: "--contract v1 needs the command's own --json flag.",
    },
    CodeInfo {
        code: "input_not_found",
        kind: ErrorKind::Input,
        retryable: false,
        summary: "The input path does not exist or cannot be read.",
    },
    CodeInfo {
        code: "input_unsupported_format",
        kind: ErrorKind::Input,
        retryable: false,
        summary: "The input is not a supported executable format.",
    },
    CodeInfo {
        code: "resource_root_missing",
        kind: ErrorKind::Environment,
        retryable: false,
        summary: "Required resources (signatures, detectors) were not found.",
    },
    CodeInfo {
        code: "function_timeout",
        kind: ErrorKind::Timeout,
        retryable: true,
        summary: "A configured time limit was reached before a result was produced.",
    },
    CodeInfo {
        code: "all_items_failed",
        kind: ErrorKind::Failed,
        retryable: false,
        summary: "A batch ran but every requested item failed; data lists them.",
    },
    CodeInfo {
        code: "partial_failure",
        kind: ErrorKind::Partial,
        retryable: false,
        summary: "Some batch items failed and --fail-on-partial was given; data has all.",
    },
    CodeInfo {
        code: "output_not_json",
        kind: ErrorKind::Failed,
        retryable: false,
        summary: "The command succeeded but its stdout was not JSON.",
    },
    CodeInfo {
        code: "internal_panic",
        kind: ErrorKind::Internal,
        retryable: false,
        summary: "The command terminated abnormally (panic or crash).",
    },
    CodeInfo {
        code: "function_not_found",
        kind: ErrorKind::Input,
        retryable: false,
        summary: "No discovered function has the requested name.",
    },
    CodeInfo {
        code: "function_ambiguous",
        kind: ErrorKind::Input,
        retryable: false,
        summary: "The requested name stands for several function addresses; pass --addr.",
    },
    CodeInfo {
        code: "quality_budget_exceeded",
        kind: ErrorKind::Failed,
        retryable: false,
        summary: "A sandbox run exceeded a quality budget (unmodelled libc or syscalls); the report was still printed.",
    },
    CodeInfo {
        code: "unclassified",
        kind: ErrorKind::Failed,
        retryable: false,
        summary: "The command failed with an error that has no specific code yet.",
    },
];

pub fn lookup(code: &str) -> Option<&'static CodeInfo> {
    ERROR_CODES.iter().find(|info| info.code == code)
}

/// The registry as JSON, for `fission_cli schema --error-codes`.
pub fn registry_json() -> Value {
    let codes: Vec<Value> = ERROR_CODES
        .iter()
        .map(|info| {
            json!({
                "code": info.code,
                "kind": info.kind.as_str(),
                "exit_code": info.kind.exit_code(),
                "retryable": info.retryable,
                "summary": info.summary,
            })
        })
        .collect();
    json!({ "contract": super::CONTRACT_ID, "error_codes": codes })
}

/// A `--fail-on-budget` sandbox run went over its HLE or opcode budget.
///
/// Carried as a typed error so the code does not depend on the message text.
#[derive(Debug)]
pub(crate) struct QualityBudgetExceeded(pub String);

impl std::fmt::Display for QualityBudgetExceeded {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for QualityBudgetExceeded {}

/// A `--name` selector that did not resolve to exactly one function.
#[derive(Debug)]
pub(crate) struct FunctionSelectorError(pub crate::cli::oneshot::NameLookupError);

impl std::fmt::Display for FunctionSelectorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::error::Error for FunctionSelectorError {}

/// Pick a registry code for an error raised inside the CLI.
///
/// Classification is by error type first and message text second. Text
/// matching is a stopgap until each source returns a typed error; anything
/// unrecognised maps to `unclassified` rather than guessing.
pub fn classify_error(error: &anyhow::Error) -> &'static str {
    for cause in error.chain() {
        if let Some(io) = cause.downcast_ref::<std::io::Error>()
            && io.kind() == std::io::ErrorKind::NotFound
        {
            return "input_not_found";
        }
    }
    if let Some(selector) = error.downcast_ref::<FunctionSelectorError>() {
        return match selector.0 {
            crate::cli::oneshot::NameLookupError::NotFound(_) => "function_not_found",
            crate::cli::oneshot::NameLookupError::Ambiguous(..) => "function_ambiguous",
        };
    }
    if error.downcast_ref::<QualityBudgetExceeded>().is_some() {
        return "quality_budget_exceeded";
    }
    let by_text = classify_message(&format!("{error:#}"));
    if by_text != "unclassified" {
        return by_text;
    }
    // The loader is the only owner of `FissionError::Loader`: it means the bytes
    // could not be turned into a binary (unknown magic, truncated header, a range
    // outside the file). That is a property of the input, whatever the wording.
    let loader_rejected = error.chain().any(|cause| {
        matches!(
            cause.downcast_ref::<fission_core::FissionError>(),
            Some(fission_core::FissionError::Loader(_))
        )
    });
    if loader_rejected {
        "input_unsupported_format"
    } else {
        "unclassified"
    }
}

pub(crate) fn classify_message(message: &str) -> &'static str {
    let lower = message.to_lowercase();
    if lower.contains("does not exist") || lower.contains("no such file") {
        "input_not_found"
    } else if lower.contains("not in recognized object file format")
        || lower.contains("unknown file magic")
        || lower.contains("unrecognized input format")
        || lower.contains("unsupported binary format")
    {
        "input_unsupported_format"
    } else if lower.contains("preview_timeout") || lower.contains("timed out") {
        "function_timeout"
    } else if lower.contains("resource")
        && (lower.contains("not found") || lower.contains("missing"))
    {
        "resource_root_missing"
    } else {
        "unclassified"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exit_codes_are_distinct_and_documented() {
        let kinds = [
            ErrorKind::Failed,
            ErrorKind::Usage,
            ErrorKind::Input,
            ErrorKind::Partial,
            ErrorKind::Timeout,
            ErrorKind::Environment,
            ErrorKind::Internal,
        ];
        let codes: Vec<i32> = kinds.iter().map(|kind| kind.exit_code()).collect();
        assert_eq!(codes, vec![1, 2, 3, 4, 5, 6, 70]);
    }

    #[test]
    fn registry_codes_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for info in ERROR_CODES {
            assert!(seen.insert(info.code), "duplicate code {}", info.code);
        }
    }

    #[test]
    fn classifies_the_observed_messages() {
        assert_eq!(
            classify_message("binary path does not exist: C:\\nope.exe"),
            "input_not_found"
        );
        assert_eq!(
            classify_message("not in recognized object file format: Unknown file magic"),
            "input_unsupported_format"
        );
        assert_eq!(
            classify_message("preview_timeout: Rust-Sleigh render timed out after 500ms"),
            "function_timeout"
        );
        assert_eq!(classify_message("something else entirely"), "unclassified");
    }

    #[test]
    fn loader_rejections_are_unsupported_input_not_unclassified() {
        // The loader raises `FissionError::Loader` with these texts for a file that
        // is not a loadable executable; the CLI wraps them in "failed to parse
        // binary". None of them contains a phrase the text classifier knows.
        for message in [
            "UnsupportedFormat: unknown binary format",
            "Binary too small",
            "byte range out of bounds at 0x14",
        ] {
            let error = anyhow::Error::new(fission_core::FissionError::loader(message))
                .context("failed to parse binary `/tmp/x`");
            assert_eq!(
                classify_error(&error),
                "input_unsupported_format",
                "{message}"
            );
        }
    }

    #[test]
    fn a_non_loader_error_stays_unclassified() {
        let error = anyhow::Error::new(fission_core::FissionError::Other("boom".into()));
        assert_eq!(classify_error(&error), "unclassified");
    }

    #[test]
    fn a_sandbox_budget_overrun_has_its_own_code() {
        let error = anyhow::Error::new(QualityBudgetExceeded(
            "HLE quality budget exceeded: hle_misses=5 (max 0)".into(),
        ));
        assert_eq!(classify_error(&error), "quality_budget_exceeded");
        let info = lookup("quality_budget_exceeded").expect("registered");
        assert_eq!(info.kind, ErrorKind::Failed);
    }

    #[test]
    fn io_not_found_is_input_not_found() {
        let error = anyhow::Error::new(std::io::Error::from(std::io::ErrorKind::NotFound))
            .context("opening binary");
        assert_eq!(classify_error(&error), "input_not_found");
    }
}
