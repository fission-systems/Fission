//! fission-sleigh build script.
//!
//! Resolves compile-time resources declared in `build.toml` and copies them
//! into `OUT_DIR` so that `include_str!(env!("..."))` works regardless of
//! where the workspace root lives (monorepo, Cargo git dep, CI, cross-build).
//!
//! # Resource resolution order
//!
//! 1. `FISSION_SLEIGH_SPEC_DIR` env var  
//!    Set this to the `sleigh-specs/` directory in CI or when building
//!    `fission-web` (where utils/ is not at the monorepo root).
//!
//! 2. Workspace root `utils/sleigh-specs/`  
//!    Detected by walking up from `CARGO_MANIFEST_DIR` until a `Cargo.lock`
//!    is found — the standard Fission monorepo layout.
//!
//! # Build config
//!
//! `build.toml` (alongside this file) is the authoritative source of resource
//! declarations. Changing a path or env var there is the single place of truth.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

// ── Build config (parsed from build.toml at compile time) ───────────────────

/// Minimal TOML value we need — avoids a full TOML parse dep at build time.
/// We only need two string keys from a single `[resources.language_manifest]`
/// table, so a hand-rolled extractor is simpler than pulling in `toml`.
struct BuildConfig {
    env_var:            String,
    env_subpath:        String,
    workspace_relative: String,
}

impl BuildConfig {
    fn load() -> Self {
        let src = include_str!("build.toml");

        let env_var            = extract(src, "env_var");
        let env_subpath        = extract(src, "env_subpath");
        let workspace_relative = extract(src, "workspace_relative");

        Self { env_var, env_subpath, workspace_relative }
    }
}

/// Extracts a quoted string value for `key = "value"` from raw TOML text.
fn extract(toml: &str, key: &str) -> String {
    for line in toml.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix(key) {
            let rest = rest.trim();
            if let Some(rest) = rest.strip_prefix('=') {
                let rest = rest.trim().trim_matches('"');
                return rest.to_string();
            }
        }
    }
    panic!("build.toml: missing key `{key}`");
}

// ── Workspace root detection ─────────────────────────────────────────────────

/// Walks up from `start` until a `Cargo.lock` is found (workspace root).
fn find_workspace_root(start: &Path) -> Option<PathBuf> {
    let mut dir = start.to_path_buf();
    loop {
        if dir.join("Cargo.lock").exists() {
            return Some(dir);
        }
        if !dir.pop() {
            return None;
        }
    }
}

// ── Main ─────────────────────────────────────────────────────────────────────

fn main() {
    let cfg = BuildConfig::load();

    let out_dir      = PathBuf::from(env::var("OUT_DIR").unwrap());
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());

    // Tell cargo to rerun if the config or the env var changes.
    println!("cargo:rerun-if-changed=build.toml");
    println!("cargo:rerun-if-env-changed={}", cfg.env_var);

    // ── 1. FISSION_SLEIGH_SPEC_DIR override ──────────────────────────────
    let source: PathBuf = if let Ok(spec_dir) = env::var(&cfg.env_var) {
        let candidate = PathBuf::from(&spec_dir).join(&cfg.env_subpath);
        if candidate.exists() {
            println!(
                "cargo:warning=fission-sleigh: using {} from ${}",
                cfg.env_subpath, cfg.env_var
            );
            candidate
        } else {
            panic!(
                "\n\nfission-sleigh build error:\n  \
                 ${} is set to `{spec_dir}` but `{}` was not found there.\n  \
                 Expected: {}\n",
                cfg.env_var, cfg.env_subpath,
                PathBuf::from(&spec_dir).join(&cfg.env_subpath).display()
            );
        }

    // ── 2. Workspace root utils/ fallback ────────────────────────────────
    } else if let Some(ws_root) = find_workspace_root(&manifest_dir) {
        let candidate = ws_root.join(&cfg.workspace_relative);
        if candidate.exists() {
            println!("cargo:rerun-if-changed={}", candidate.display());
            candidate
        } else {
            panic!(
                "\n\nfission-sleigh build error:\n  \
                 `{}` not found.\n  \
                 Either:\n  \
                   a) Set ${} to the sleigh-specs/ directory, or\n  \
                   b) Download fission-utils and extract to `{}/utils/`\n  \
                      from: https://github.com/fission-systems/Fission/releases\n",
                candidate.display(),
                cfg.env_var,
                ws_root.display()
            );
        }

    // ── 3. Neither found ─────────────────────────────────────────────────
    } else {
        panic!(
            "\n\nfission-sleigh build error:\n  \
             Could not locate `{}`.\n  \
             Set ${} to the sleigh-specs/ directory.\n",
            cfg.workspace_relative, cfg.env_var
        );
    };

    // Copy manifest to OUT_DIR so include_str!(env!(...)) resolves correctly
    // regardless of the working directory at compile time.
    let dest = out_dir.join("ghidra_language_manifest.json");
    fs::copy(&source, &dest).unwrap_or_else(|e| {
        panic!(
            "fission-sleigh build error: failed to copy manifest\n  \
             from: {}\n  to:   {}\n  error: {e}",
            source.display(), dest.display()
        )
    });

    // Export the absolute path so registry.rs can include_str! it.
    println!("cargo:rustc-env=FISSION_MANIFEST_JSON={}", dest.display());
}
