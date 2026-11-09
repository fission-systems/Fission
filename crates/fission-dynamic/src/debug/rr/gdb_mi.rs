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
            let content = self.unescape_string(Self::between(s, 1, end));
            (MiValue::Const(content), Self::after(s, end))
        } else if s.starts_with('{') {
            // Tuple: {name="value",...}
            // An unterminated tuple (a truncated record) runs to the end of the
            // input rather than past it.
            let end = self.find_matching_brace(s, '{', '}').unwrap_or(s.len());
            let results = self.parse_results(Self::between(s, 1, end));
            (MiValue::Tuple(results), Self::after(s, end))
        } else if s.starts_with('[') {
            // List: [value1,value2,...] or [name="value",...]
            let end = self.find_matching_brace(s, '[', ']').unwrap_or(s.len());
            let content = &Self::between(s, 1, end).trim();

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
            (MiValue::List(list), Self::after(s, end))
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
        // Unterminated: the string runs to the end of the input.
        s.len()
    }

    /// `s[start..end]`, empty when the range is inverted or off the end.
    fn between(s: &str, start: usize, end: usize) -> &str {
        s.get(start..end.max(start)).unwrap_or("")
    }

    /// What follows the delimiter at `end`, empty when there is no more input.
    fn after(s: &str, end: usize) -> &str {
        s.get(end + 1..).unwrap_or("")
    }

    /// Index of the `close` that matches the `open` the text starts with.
    ///
    /// Delimiters inside a quoted string (`{msg="a } b"}`) are text, not
    /// structure, and a `close` seen with nothing open is ignored rather than
    /// subtracted from zero.
    fn find_matching_brace(&self, s: &str, open: char, close: char) -> Option<usize> {
        let mut depth = 0usize;
        let mut in_string = false;
        let mut escaped = false;
        for (i, c) in s.char_indices() {
            if in_string {
                if escaped {
                    escaped = false;
                } else if c == '\\' {
                    escaped = true;
                } else if c == '"' {
                    in_string = false;
                }
            } else if c == '"' {
                in_string = true;
            } else if c == open {
                depth += 1;
            } else if c == close && depth > 0 {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
        }
        None
    }

    fn unescape_string(&self, s: &str) -> String {
        // One left-to-right pass: each backslash escape is read exactly once,
        // so the output of one replacement can never be read as the start of
        // another (`\\` followed by `n` is a backslash and an `n`).
        let mut out = String::with_capacity(s.len());
        let mut chars = s.chars();
        while let Some(c) = chars.next() {
            if c != '\\' {
                out.push(c);
                continue;
            }
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('"') => out.push('"'),
                Some('\\') => out.push('\\'),
                // Unknown escape: keep it as written.
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            }
        }
        out
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

    fn first_result(line: &str) -> HashMap<String, MiValue> {
        let mut parser = GdbMiParser::new();
        parser.feed(line);
        match parser.parse().into_iter().next() {
            Some(MiResponse::Result { results, .. }) => results,
            other => panic!("expected a result record, got {other:?}"),
        }
    }

    fn tuple(value: &MiValue) -> &HashMap<String, MiValue> {
        match value {
            MiValue::Tuple(map) => map,
            other => panic!("expected a tuple, got {other:?}"),
        }
    }

    fn constant(value: &MiValue) -> &str {
        match value {
            MiValue::Const(text) => text,
            other => panic!("expected a constant, got {other:?}"),
        }
    }

    #[test]
    fn a_closing_brace_inside_a_string_does_not_end_the_tuple() {
        // `msg` contains `}` and `]`; both must stay inside the string, and
        // `line` after it must still be parsed.
        let results = first_result("^done,frame={msg=\"a } b ] c\",line=\"7\"}\n");
        let frame = tuple(&results["frame"]);
        assert_eq!(constant(&frame["msg"]), "a } b ] c");
        assert_eq!(constant(&frame["line"]), "7");
    }

    #[test]
    fn nested_tuples_and_lists_still_parse() {
        // Positive control for the test above: ordinary nesting is unchanged.
        let results = first_result("^done,frame={a=\"1\",b={c=\"2\"},d=[\"x\",\"y\"]}\n");
        let frame = tuple(&results["frame"]);
        assert_eq!(constant(&frame["a"]), "1");
        assert_eq!(constant(&tuple(&frame["b"])["c"]), "2");
        match &frame["d"] {
            MiValue::List(items) => {
                assert_eq!(items.iter().map(constant).collect::<Vec<_>>(), ["x", "y"])
            }
            other => panic!("expected a list, got {other:?}"),
        }
    }

    #[test]
    fn truncated_records_do_not_panic() {
        // A record cut off mid-value (a partial read, a crashed gdb) used to
        // slice with its start past its end.
        for line in [
            "^done,msg=\"\n",
            "^done,frame={\n",
            "^done,files=[\n",
            "^done,frame={a=\"1\"\n",
            "^done,x={[\"\n",
        ] {
            let mut parser = GdbMiParser::new();
            parser.feed(line);
            let _ = parser.parse();
        }
    }

    #[test]
    fn unescape_reads_each_escape_once_left_to_right() {
        let parser = GdbMiParser::new();
        // An escaped backslash followed by `n` is a backslash and an `n`, not
        // a backslash and a newline.
        assert_eq!(parser.unescape_string(r"a\\nb"), r"a\nb");
        // An escaped backslash before a quote leaves the quote's own escape
        // alone: `\\` then `\"` is a backslash and a quote.
        assert_eq!(parser.unescape_string(r#"x\\\"y"#), r#"x\"y"#);
        // The plain escapes still work.
        assert_eq!(parser.unescape_string(r#"l1\nl2\tq\"e"#), "l1\nl2\tq\"e");
        assert_eq!(parser.unescape_string("no escapes"), "no escapes");
        // A trailing lone backslash is kept, not dropped.
        assert_eq!(parser.unescape_string("end\\"), "end\\");
    }
}
