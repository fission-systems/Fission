//! Mock AI provider for testing and offline development.

use async_trait::async_trait;

use super::{AiProvider, ChunkStream, ProviderResult, ResponseChunk};
use crate::session::Message;
use crate::tools::ToolDefinition;

#[derive(Debug)]
pub struct MockProvider {
    model: String,
}

impl MockProvider {
    pub fn new(model: String) -> Self {
        Self { model }
    }
}

fn extract_function_name(code: &str) -> String {
    for line in code.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("//") || trimmed.starts_with("/*") || trimmed.starts_with("*") {
            continue;
        }
        if let Some(open_paren_idx) = trimmed.find('(') {
            let before_paren = trimmed[..open_paren_idx].trim();
            let is_control = ["if", "while", "for", "switch", "return"]
                .iter()
                .any(|&kw| {
                    before_paren == kw
                        || before_paren.ends_with(&format!(" {}", kw))
                        || before_paren.ends_with(&format!("\t{}", kw))
                });
            if is_control {
                continue;
            }
            let words: Vec<&str> = before_paren
                .split(|c: char| !c.is_alphanumeric() && c != '_')
                .filter(|s| !s.is_empty())
                .collect();
            if let Some(last_word) = words.last()
                && last_word
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_alphabetic() || c == '_')
            {
                return last_word.to_string();
            }
        }
    }
    "unknown_function".to_string()
}

fn extract_variables(code: &str) -> Vec<String> {
    use std::collections::HashSet;
    let mut vars = HashSet::new();
    for word in code.split(|c: char| !c.is_alphanumeric() && c != '_') {
        if word.starts_with("param_")
            || word.starts_with("local_")
            || word.starts_with("uVar")
            || word.starts_with("iVar")
        {
            vars.insert(word.to_string());
        }
    }
    let mut vars_list: Vec<String> = vars.into_iter().collect();
    vars_list.sort();
    if vars_list.is_empty() {
        vars_list.push("param_1".to_string());
        vars_list.push("local_1".to_string());
    }
    vars_list
}

pub fn generate_mock_report(code: &str) -> String {
    let function_name = extract_function_name(code);
    let variables = extract_variables(code);

    let mut report = String::new();
    report.push_str("# Decompiled Function Analysis\n\n");
    report.push_str(&format!("- **Target Function**: {}\n", function_name));
    report.push_str("- **Key Variables Identified**:\n");
    for var in &variables {
        report.push_str(&format!("  - `{}`\n", var));
    }
    report.push_str("\n- **Analysis Summary**:\n");
    report.push_str(&format!(
        "  The function `{}` was analyzed. It contains control flow and operations involving variables like {}. The logic suggests it functions as a helper or key component of the module.\n",
        function_name,
        variables.join(", ")
    ));
    report.push_str("\n- **Security Assessment**:\n");
    report.push_str("  No obvious buffer overflows or injection vulnerabilities were detected from static analysis. Recommend checking boundary bounds on variables.");
    report
}

#[async_trait]
impl AiProvider for MockProvider {
    fn name(&self) -> &str {
        "mock"
    }

    fn model(&self) -> &str {
        &self.model
    }

    fn requires_auth(&self) -> bool {
        false
    }

    async fn chat_stream(
        &self,
        messages: &[Message],
        _tools: Option<&[ToolDefinition]>,
    ) -> ProviderResult<ChunkStream> {
        let code = messages
            .iter()
            .find(|m| m.role == crate::session::Role::User)
            .and_then(|m| m.content.as_deref())
            .unwrap_or("");

        let report = generate_mock_report(code);
        let mut chunks = Vec::new();
        let lines: Vec<&str> = report.split('\n').collect();
        for (i, line) in lines.iter().enumerate() {
            let mut delta = line.to_string();
            if i < lines.len() - 1 {
                delta.push('\n');
            }
            chunks.push(Ok(ResponseChunk {
                delta,
                tool_calls: None,
                done: i == lines.len() - 1,
            }));
        }

        let stream = futures::stream::iter(chunks);
        Ok(Box::pin(stream))
    }

    async fn chat(
        &self,
        messages: &[Message],
        _tools: Option<&[ToolDefinition]>,
    ) -> ProviderResult<String> {
        let code = messages
            .iter()
            .find(|m| m.role == crate::session::Role::User)
            .and_then(|m| m.content.as_deref())
            .unwrap_or("");

        Ok(generate_mock_report(code))
    }
}
