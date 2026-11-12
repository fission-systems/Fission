//! `fission_cli skill`: the agent usage guide, compiled into the binary.
//!
//! The text is `skills/fission/SKILL.md`, so the guide an agent reads is the one
//! that shipped with the build it is about to drive. The tests below hold the
//! guide to the CLI: every command and flag it shows must exist, so a renamed
//! flag fails here instead of in an agent's session.

use anyhow::Result;
use std::io::Write;

/// The guide, as written in the repository.
pub const SKILL_MD: &str = include_str!("../../../../skills/fission/SKILL.md");

pub fn run_skill() -> Result<()> {
    let mut out = std::io::stdout().lock();
    out.write_all(SKILL_MD.as_bytes())?;
    if !SKILL_MD.ends_with('\n') {
        out.write_all(b"\n")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    /// Logical command lines from the guide's fenced blocks, with `\` continuations
    /// joined. Anything after a pipe, redirect or comment is not the CLI's.
    fn cli_invocations(markdown: &str) -> Vec<Vec<String>> {
        let mut lines = Vec::new();
        let mut in_fence = false;
        let mut pending = String::new();
        for raw in markdown.lines() {
            if raw.trim_start().starts_with("```") {
                in_fence = !in_fence;
                pending.clear();
                continue;
            }
            if !in_fence {
                continue;
            }
            let trimmed = raw.trim();
            if let Some(stripped) = trimmed.strip_suffix('\\') {
                pending.push_str(stripped);
                pending.push(' ');
                continue;
            }
            pending.push_str(trimmed);
            lines.push(std::mem::take(&mut pending));
        }
        let mut invocations = Vec::new();
        for line in lines {
            let Some(at) = line.find("fission_cli") else {
                continue;
            };
            let mut tokens = Vec::new();
            for token in line[at + "fission_cli".len()..].split_whitespace() {
                if matches!(token, "|" | ">" | ">>" | "&&" | "||" | ";") || token.starts_with('#') {
                    break;
                }
                tokens.push(token.to_string());
            }
            invocations.push(tokens);
        }
        invocations
    }

    fn long_flags(command: &clap::Command) -> HashSet<String> {
        command
            .get_arguments()
            .filter_map(|arg| arg.get_long().map(str::to_string))
            .collect()
    }

    /// Every `fission_cli ...` line in the guide parses against the real clap tree:
    /// the subcommand exists, a nested subcommand exists, and each `--flag` belongs
    /// to a command on that path (or is global).
    #[test]
    fn every_command_and_flag_in_the_guide_exists() {
        let mut root = crate::cli::args::command_tree();
        root.build();
        let invocations = cli_invocations(SKILL_MD);
        assert!(
            invocations.len() >= 20,
            "the guide should show many invocations, parsed only {}",
            invocations.len()
        );

        for tokens in invocations {
            let shown = tokens.join(" ");
            let mut allowed = long_flags(&root);
            let mut current = root.clone();
            let mut iter = tokens.iter().peekable();
            let mut after_double_dash = false;
            let mut value_expected = false;
            while let Some(token) = iter.next() {
                if std::mem::take(&mut value_expected) {
                    continue;
                }
                if after_double_dash {
                    break;
                }
                if token == "--" {
                    after_double_dash = true;
                    continue;
                }
                if let Some(flag) = token.strip_prefix("--") {
                    let name = flag.split('=').next().unwrap_or(flag);
                    assert!(
                        allowed.contains(name),
                        "`--{name}` is not a flag of `{}` (guide line: fission_cli {shown})",
                        current.get_name()
                    );
                    // `--contract v1`: the value is not a subcommand. Only flags
                    // written without `=` can leave a value to skip.
                    value_expected = !flag.contains('=')
                        && current
                            .get_arguments()
                            .find(|arg| arg.get_long() == Some(name))
                            .is_some_and(|arg| arg.get_action().takes_values())
                        && current.get_name() == "fission_cli";
                    continue;
                }
                if token.starts_with('-') || token.starts_with('.') || token.starts_with('/') {
                    continue;
                }
                // A bare word: a subcommand while the current command has any,
                // otherwise a positional value (path, address, name).
                if let Some(sub) = current.find_subcommand(token) {
                    let mut sub = sub.clone();
                    sub.build();
                    allowed.extend(long_flags(&sub));
                    current = sub;
                } else if current.has_subcommands() && current.get_name() == "fission_cli" {
                    panic!(
                        "`{token}` is not a fission_cli subcommand (guide line: fission_cli {shown})"
                    );
                }
            }
        }
    }

    #[test]
    fn the_guide_has_the_frontmatter_a_skill_loader_needs() {
        assert!(SKILL_MD.starts_with("---\nname: fission-cli\n"));
        let end = SKILL_MD[4..].find("\n---\n").expect("closing frontmatter");
        assert!(SKILL_MD[4..4 + end].contains("description:"));
    }

    /// The codes an agent is told to branch on must exist in the registry, so a
    /// retired or misspelled code cannot linger in the guide.
    #[test]
    fn every_error_code_the_guide_branches_on_is_registered() {
        let registered: HashSet<&str> = crate::cli::contract::ERROR_CODES
            .iter()
            .map(|info| info.code)
            .collect();
        let named = [
            "usage_error",
            "json_required",
            "input_not_found",
            "input_unsupported_format",
            "resource_root_missing",
            "function_timeout",
            "all_items_failed",
            "partial_failure",
            "function_not_found",
            "function_ambiguous",
            "quality_budget_exceeded",
            "unclassified",
        ];
        for code in named {
            assert!(registered.contains(code), "{code} is not registered");
            assert!(
                SKILL_MD.contains(&format!("`{code}`")),
                "the guide does not mention `{code}`"
            );
        }
    }
}
