//! Just enough of a reader for the GitHub workflow files to pull out the
//! values `preflight` must share with CI: a job's `with:` inputs, a step's
//! `run:` text and `env:` keys, an input's default and a workflow's `env:`.
//!
//! It understands block-indented YAML as the workflows in this repository
//! write it, nothing more. A value it cannot find is an error, never a
//! default, so a renamed step or input fails `preflight` and its tests
//! instead of silently running a different command from CI.

/// Indentation of a line in columns.
fn indent(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

fn is_blank(line: &str) -> bool {
    let t = line.trim();
    t.is_empty() || t.starts_with('#')
}

/// Strip one pair of surrounding quotes and a trailing ` # comment`.
fn scalar(raw: &str) -> String {
    let mut v = raw.trim();
    if let Some(pos) = v.find(" #") {
        if !v.starts_with('"') && !v.starts_with('\'') {
            v = v[..pos].trim_end();
        }
    }
    if v.len() >= 2
        && ((v.starts_with('"') && v.ends_with('"')) || (v.starts_with('\'') && v.ends_with('\'')))
    {
        v = &v[1..v.len() - 1];
    }
    v.to_string()
}

/// The lines below `lines[header]` that are indented deeper than it.
fn children<'a>(lines: &[&'a str], header: usize) -> Vec<&'a str> {
    let base = indent(lines[header]);
    lines[header + 1..]
        .iter()
        .take_while(|l| is_blank(l) || indent(l) > base)
        .copied()
        .collect()
}

/// `key: value` entries directly under the mapping in `body` (one indent level).
fn mapping(body: &[&str]) -> Vec<(String, String)> {
    let level = body
        .iter()
        .filter(|l| !is_blank(l))
        .map(|l| indent(l))
        .min();
    let Some(level) = level else {
        return Vec::new();
    };
    body.iter()
        .filter(|l| !is_blank(l) && indent(l) == level)
        .filter_map(|l| {
            let t = l.trim();
            let (k, v) = t.split_once(':')?;
            Some((k.trim().to_string(), scalar(v)))
        })
        .collect()
}

/// First line index at `indent == col` whose trimmed text equals `text`.
fn find_line(lines: &[&str], col: Option<usize>, text: &str) -> Option<usize> {
    lines
        .iter()
        .position(|l| l.trim() == text && col.is_none_or(|c| indent(l) == c))
}

/// A job's `with:` inputs (`jobs.<job_id>.with`).
pub fn job_with(yaml: &str, job_id: &str) -> Result<Vec<(String, String)>, String> {
    let lines: Vec<&str> = yaml.lines().collect();
    let job = find_line(&lines, Some(2), &format!("{job_id}:"))
        .ok_or_else(|| format!("job `{job_id}` not found"))?;
    let body = children(&lines, job);
    let with = find_line(&body, Some(4), "with:")
        .ok_or_else(|| format!("job `{job_id}` has no `with:`"))?;
    Ok(mapping(&children(&body, with)))
}

/// One `with:` input of a job.
pub fn job_input(yaml: &str, job_id: &str, key: &str) -> Result<String, String> {
    job_with(yaml, job_id)?
        .into_iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v)
        .ok_or_else(|| format!("job `{job_id}` passes no `{key}`"))
}

/// `on.workflow_call.inputs.<name>.default`.
pub fn input_default(yaml: &str, name: &str) -> Result<String, String> {
    let lines: Vec<&str> = yaml.lines().collect();
    let inputs = find_line(&lines, None, "inputs:").ok_or("workflow has no `inputs:`")?;
    let body = children(&lines, inputs);
    let input = find_line(&body, None, &format!("{name}:"))
        .ok_or_else(|| format!("input `{name}` not found"))?;
    mapping(&children(&body, input))
        .into_iter()
        .find(|(k, _)| k == "default")
        .map(|(_, v)| v)
        .ok_or_else(|| format!("input `{name}` has no default"))
}

/// The workflow-level `env:` mapping.
pub fn workflow_env(yaml: &str) -> Result<Vec<(String, String)>, String> {
    let lines: Vec<&str> = yaml.lines().collect();
    let env = find_line(&lines, Some(0), "env:").ok_or("workflow has no top-level `env:`")?;
    Ok(mapping(&children(&lines, env)))
}

/// A step, found by its `name:`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    /// The `if:` condition, empty when the step always runs.
    pub condition: String,
    /// `run:` text, one entry per line, block scalars dedented.
    pub run: Vec<String>,
    /// `env:` keys and their (unevaluated) values.
    pub env: Vec<(String, String)>,
    /// `continue-on-error:` is set to anything but `false`: the step's
    /// failure no longer fails the job.
    pub continue_on_error: bool,
}

pub fn step(yaml: &str, name: &str) -> Result<Step, String> {
    let lines: Vec<&str> = yaml.lines().collect();
    let header = format!("- name: {name}");
    let at = lines
        .iter()
        .position(|l| l.trim() == header)
        .ok_or_else(|| format!("step `{name}` not found"))?;
    let dash = indent(lines[at]);
    let body: Vec<&str> = lines[at + 1..]
        .iter()
        .take_while(|l| is_blank(l) || indent(l) > dash)
        .copied()
        .collect();
    let key_col = dash + 2;
    let mut condition = String::new();
    let mut run = None;
    let mut env = Vec::new();
    let mut continue_on_error = false;
    for (i, line) in body.iter().enumerate() {
        if is_blank(line) || indent(line) != key_col {
            continue;
        }
        let t = line.trim();
        if let Some(v) = t.strip_prefix("if:") {
            condition = v.trim().to_string();
        } else if let Some(v) = t.strip_prefix("continue-on-error:") {
            continue_on_error = scalar(v) != "false";
        } else if t == "env:" {
            env = mapping(&children(&body, i));
        } else if let Some(v) = t.strip_prefix("run:") {
            let v = v.trim();
            if v == "|" || v == "|-" || v == ">" {
                let block = children(&body, i);
                let col = block
                    .iter()
                    .filter(|l| !l.trim().is_empty())
                    .map(|l| indent(l))
                    .min()
                    .unwrap_or(0);
                let mut text: Vec<String> = block
                    .iter()
                    .map(|l| {
                        if l.len() >= col {
                            l[col..].to_string()
                        } else {
                            String::new()
                        }
                    })
                    .collect();
                while text.last().is_some_and(|l| l.trim().is_empty()) {
                    text.pop();
                }
                run = Some(text);
            } else {
                run = Some(vec![scalar(v)]);
            }
        }
    }
    let run = run.ok_or_else(|| format!("step `{name}` has no `run:`"))?;
    Ok(Step {
        condition,
        run,
        env,
        continue_on_error,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"name: Sample
on:
  workflow_call:
    inputs:
      retries:
        type: number
        default: 1
env:
  FOO: "1"
  BAR: two # comment
jobs:
  lint:
    uses: ./x.yml
    with:
      crates: ${{ needs.changes.outputs.test_crates }}
      exclude_crates: "fission-signatures"
  other:
    steps:
      - name: One line
        if: ${{ inputs.flag }}
        env:
          CRATES: ${{ inputs.crates }}
        run: bash scripts/ci/clippy.sh

      - name: Block
        continue-on-error: true
        run: |
          set -euo pipefail
          ./scripts/check/x.sh

      - name: After
        run: echo after
"#;

    #[test]
    fn reads_job_inputs() {
        assert_eq!(
            job_input(SAMPLE, "lint", "exclude_crates").unwrap(),
            "fission-signatures"
        );
        assert_eq!(
            job_input(SAMPLE, "lint", "crates").unwrap(),
            "${{ needs.changes.outputs.test_crates }}"
        );
        // A missing input is an error, not an empty default.
        assert!(job_input(SAMPLE, "lint", "nope").is_err());
        assert!(job_input(SAMPLE, "missing", "crates").is_err());
    }

    #[test]
    fn reads_input_default_and_workflow_env() {
        assert_eq!(input_default(SAMPLE, "retries").unwrap(), "1");
        let env = workflow_env(SAMPLE).unwrap();
        assert_eq!(
            env,
            vec![
                ("FOO".to_string(), "1".to_string()),
                ("BAR".to_string(), "two".to_string())
            ]
        );
    }

    #[test]
    fn reads_steps() {
        let one = step(SAMPLE, "One line").unwrap();
        assert_eq!(one.run, vec!["bash scripts/ci/clippy.sh"]);
        assert_eq!(one.condition, "${{ inputs.flag }}");
        assert_eq!(one.env[0].0, "CRATES");
        let block = step(SAMPLE, "Block").unwrap();
        assert_eq!(
            block.run,
            vec!["set -euo pipefail", "./scripts/check/x.sh"],
            "a block scalar stops at the next step"
        );
        assert!(block.continue_on_error);
        assert!(
            !one.continue_on_error,
            "absent means the step fails the job"
        );
        assert!(step(SAMPLE, "Nope").is_err());
    }
}
