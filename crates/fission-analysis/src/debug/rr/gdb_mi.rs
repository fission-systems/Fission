//! GDB/MI Protocol Parser
//!
//! Parses responses from GDB Machine Interface (MI) protocol.
//! RR's replay mode runs under GDB, so we communicate via MI.

use std::collections::HashMap;

/// GDB/MI response types
#[derive(Debug, Clone)]
pub enum MiResponse {
    /// Result record (^done, ^running, ^error, etc.)
    Result {
        token: Option<u32>,
        class: String,
        results: HashMap<String, MiValue>,
    },
    /// Async exec record (*stopped, *running)
    ExecAsync {
        token: Option<u32>,
        class: String,
        results: HashMap<String, MiValue>,
    },
    /// Async status record (+...)
    StatusAsync {
        token: Option<u32>,
        class: String,
        results: HashMap<String, MiValue>,
    },
    /// Async notify record (=...)
    NotifyAsync {
        token: Option<u32>,
        class: String,
        results: HashMap<String, MiValue>,
    },
    /// Console stream output (~"...")
    ConsoleStream(String),
    /// Target stream output (@"...")
    TargetStream(String),
    /// Log stream output (&"...")
    LogStream(String),
    /// GDB prompt (gdb)
    Prompt,
}

/// GDB/MI value types
#[derive(Debug, Clone)]
pub enum MiValue {
    Const(String),
    Tuple(HashMap<String, MiValue>),
    List(Vec<MiValue>),
}

/// Parser for GDB/MI protocol
#[derive(Debug, Default)]
pub struct GdbMiParser {
    buffer: String,
}

impl GdbMiParser {
    pub fn new() -> Self {
        Self {
            buffer: String::new(),
        }
    }

    /// Feed data to the parser
    pub fn feed(&mut self, data: &str) {
        self.buffer.push_str(data);
    }

    /// Try to parse complete responses from the buffer
    pub fn parse(&mut self) -> Vec<MiResponse> {
        let mut responses = Vec::new();

        while let Some(line_end) = self.buffer.find('\n') {
            let line = self.buffer[..line_end].trim().to_string();
            self.buffer = self.buffer[line_end + 1..].to_string();

            if line.is_empty() {
                continue;
            }

            if let Some(resp) = self.parse_line(&line) {
                responses.push(resp);
            }
        }

        responses
    }

    /// Parse a single MI response line
    fn parse_line(&self, line: &str) -> Option<MiResponse> {
        let line = line.trim();

        // Check for prompt
        if line == "(gdb)" || line == "(gdb) " {
            return Some(MiResponse::Prompt);
        }

        // Parse token if present
        let (token, rest) = self.extract_token(line);

        // Parse based on first character
        let first = rest.chars().next()?;
        match first {
            '^' => self.parse_result_record(token, &rest[1..]),
            '*' => self.parse_exec_async(token, &rest[1..]),
            '+' => self.parse_status_async(token, &rest[1..]),
            '=' => self.parse_notify_async(token, &rest[1..]),
            '~' => self.parse_console_stream(&rest[1..]),
            '@' => self.parse_target_stream(&rest[1..]),
            '&' => self.parse_log_stream(&rest[1..]),
            _ => None,
        }
    }

    fn extract_token<'a>(&self, line: &'a str) -> (Option<u32>, &'a str) {
        let mut i = 0;
        for c in line.chars() {
            if c.is_ascii_digit() {
                i += 1;
            } else {
                break;
            }
        }

        if i > 0 {
            let token = line[..i].parse().ok();
            (token, &line[i..])
        } else {
            (None, line)
        }
    }

    fn parse_result_record(&self, token: Option<u32>, s: &str) -> Option<MiResponse> {
        let (class, results) = self.parse_class_and_results(s)?;
        Some(MiResponse::Result {
            token,
            class,
            results,
        })
    }

    fn parse_exec_async(&self, token: Option<u32>, s: &str) -> Option<MiResponse> {
        let (class, results) = self.parse_class_and_results(s)?;
        Some(MiResponse::ExecAsync {
            token,
            class,
            results,
        })
    }

    fn parse_status_async(&self, token: Option<u32>, s: &str) -> Option<MiResponse> {
        let (class, results) = self.parse_class_and_results(s)?;
        Some(MiResponse::StatusAsync {
            token,
            class,
            results,
        })
    }

    fn parse_notify_async(&self, token: Option<u32>, s: &str) -> Option<MiResponse> {
        let (class, results) = self.parse_class_and_results(s)?;
        Some(MiResponse::NotifyAsync {
            token,
            class,
            results,
        })
    }

    fn parse_class_and_results(&self, s: &str) -> Option<(String, HashMap<String, MiValue>)> {
        let comma_pos = s.find(',');
        let class = match comma_pos {
            Some(pos) => s[..pos].to_string(),
            None => s.to_string(),
        };

        let results = if let Some(pos) = comma_pos {
            self.parse_results(&s[pos + 1..])
        } else {
            HashMap::new()
        };

        Some((class, results))
    }

    fn parse_results(&self, s: &str) -> HashMap<String, MiValue> {
        // Simplified parsing - just extract key=value pairs
        let mut results = HashMap::new();
        let mut rest = s.trim();

        while !rest.is_empty() {
            if let Some(eq_pos) = rest.find('=') {
                let key = rest[..eq_pos].trim().to_string();
                let after_eq = &rest[eq_pos + 1..];

                let (value, remaining) = self.parse_value(after_eq);
                results.insert(key, value);

                rest = remaining.trim_start_matches(',').trim();
            } else {
                break;
            }
        }

        results
    }

    fn parse_value<'a>(&self, s: &'a str) -> (MiValue, &'a str) {
        let s = s.trim();

        if s.starts_with('"') {
            // String constant
            let end = self.find_string_end(s);
            let content = &s[1..end];
            let content = self.unescape_string(content);
            (MiValue::Const(content), &s[end + 1..])
        } else if s.starts_with('{') {
            // Tuple: {name="value",...}
            let end = self.find_matching_brace(s, '{', '}').unwrap_or(s.len() - 1);
            let content = &s[1..end];
            let results = self.parse_results(content);
            (MiValue::Tuple(results), &s[end + 1..])
        } else if s.starts_with('[') {
            // List: [value1,value2,...] or [name="value",...]
            let end = self.find_matching_brace(s, '[', ']').unwrap_or(s.len() - 1);
            let content = &s[1..end].trim();

            let mut list = Vec::new();
            if !content.is_empty() {
                // Lists can contain values OR result pairs
                if content.contains('=') && !content.starts_with('{') && !content.starts_with('"') {
                    // It's a list of result pairs, treat as a list containing one tuple
                    let results = self.parse_results(content);
                    list.push(MiValue::Tuple(results));
                } else {
                    // List of values
                    let mut rest = *content;
                    while !rest.is_empty() {
                        let (val, rem) = self.parse_value(rest);
                        list.push(val);
                        rest = rem.trim_start_matches(',').trim();
                    }
                }
            }
            (MiValue::List(list), &s[end + 1..])
        } else {
            // Raw identifier or number
            let end = s.find([',', '}', ']']).unwrap_or(s.len());
            (MiValue::Const(s[..end].to_string()), &s[end..])
        }
    }

    fn find_string_end(&self, s: &str) -> usize {
        let mut escaped = false;
        for (i, c) in s.char_indices().skip(1) {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                return i;
            }
        }
        s.len() - 1
    }

    fn find_matching_brace(&self, s: &str, open: char, close: char) -> Option<usize> {
        let mut depth = 0;
        for (i, c) in s.char_indices() {
            if c == open {
                depth += 1;
            } else if c == close {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
        }
        None
    }

    fn unescape_string(&self, s: &str) -> String {
        s.replace("\\n", "\n")
            .replace("\\t", "\t")
            .replace("\\\"", "\"")
            .replace("\\\\", "\\")
    }

    fn parse_console_stream(&self, s: &str) -> Option<MiResponse> {
        let content = s.trim_matches('"');
        Some(MiResponse::ConsoleStream(self.unescape_string(content)))
    }

    fn parse_target_stream(&self, s: &str) -> Option<MiResponse> {
        let content = s.trim_matches('"');
        Some(MiResponse::TargetStream(self.unescape_string(content)))
    }

    fn parse_log_stream(&self, s: &str) -> Option<MiResponse> {
        let content = s.trim_matches('"');
        Some(MiResponse::LogStream(self.unescape_string(content)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_done() {
        let mut parser = GdbMiParser::new();
        parser.feed("^done\n");
        let responses = parser.parse();
        assert_eq!(responses.len(), 1);

        if let MiResponse::Result { class, .. } = &responses[0] {
            assert_eq!(class, "done");
        } else {
            panic!("Expected Result");
        }
    }

    #[test]
    fn test_parse_stopped() {
        let mut parser = GdbMiParser::new();
        parser.feed("*stopped,reason=\"breakpoint-hit\",frame={}\n");
        let responses = parser.parse();
        assert_eq!(responses.len(), 1);

        if let MiResponse::ExecAsync { class, results, .. } = &responses[0] {
            assert_eq!(class, "stopped");
            assert!(results.contains_key("reason"));
        } else {
            panic!("Expected ExecAsync");
        }
    }
}
