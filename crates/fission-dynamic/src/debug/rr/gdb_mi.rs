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
            let list = self.parse_list(Self::between(s, 1, end));
            (MiValue::List(list), Self::after(s, end))
        } else {
            // Raw identifier or number
            let end = s.find([',', '}', ']']).unwrap_or(s.len());
            (MiValue::Const(s[..end].to_string()), &s[end..])
        }
    }

    /// The elements between a list's brackets.
    ///
    /// GDB/MI allows two kinds of list: values (`["a",{...},[...]]`) and
    /// results (`[frame={...},frame={...}]`). Each element is read for what it
    /// is, so a nested list is a list and not a key that starts with `[`. A
    /// result becomes a tuple holding that one pair: a list is where GDB
    /// repeats a key (every entry of a backtrace is `frame=`), and one map
    /// for the whole list would keep only the last of them.
    fn parse_list(&self, content: &str) -> Vec<MiValue> {
        let mut list = Vec::new();
        let mut rest = content.trim();
        while !rest.is_empty() {
            let before = rest.len();
            let (element, remaining) = self.parse_list_element(rest);
            if let Some(element) = element {
                list.push(element);
            }
            rest = remaining.trim_start();
            rest = rest.strip_prefix(',').unwrap_or(rest).trim_start();
            if rest.len() >= before {
                // Nothing was consumed: `rest` starts with a closing delimiter
                // that closes nothing (a `}` inside `[...]`). Drop it, or this
                // loop pushes empty values for ever.
                let mut chars = rest.chars();
                chars.next();
                rest = chars.as_str().trim_start();
            }
        }
        list
    }

    /// One list element and what follows it. `None` when there was no element
    /// to read at this position.
    fn parse_list_element<'a>(&self, s: &'a str) -> (Option<MiValue>, &'a str) {
        if s.starts_with(['"', '{', '[']) {
            let (value, rest) = self.parse_value(s);
            return (Some(value), rest);
        }
        // `name=value` when an `=` comes before anything that would end or
        // start a value; otherwise a bare constant.
        let stop = s
            .find([',', '}', ']', '"', '{', '[', '='])
            .unwrap_or(s.len());
        if s[stop..].starts_with('=') {
            let key = s[..stop].trim().to_string();
            let (value, rest) = self.parse_value(&s[stop + 1..]);
            let mut pair = HashMap::new();
            pair.insert(key, value);
            return (Some(MiValue::Tuple(pair)), rest);
        }
        let end = s.find([',', '}', ']']).unwrap_or(s.len());
        if end == 0 {
            return (None, s);
        }
        (Some(MiValue::Const(s[..end].trim().to_string())), &s[end..])
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

    /// The text of a stream record (`~"..."`, `@"..."`, `&"..."`).
    ///
    /// The payload is one C string. Only its own delimiters come off: trimming
    /// every quote from both ends also took the quote of a trailing `\"`,
    /// leaving a stray backslash where the text ended in a quote.
    fn stream_text(&self, s: &str) -> String {
        let s = s.trim();
        if s.starts_with('"') {
            let end = self.find_string_end(s);
            self.unescape_string(Self::between(s, 1, end))
        } else {
            self.unescape_string(s)
        }
    }

    fn parse_console_stream(&self, s: &str) -> Option<MiResponse> {
        Some(MiResponse::ConsoleStream(self.stream_text(s)))
    }

    fn parse_target_stream(&self, s: &str) -> Option<MiResponse> {
        Some(MiResponse::TargetStream(self.stream_text(s)))
    }

    fn parse_log_stream(&self, s: &str) -> Option<MiResponse> {
        Some(MiResponse::LogStream(self.stream_text(s)))
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

    fn list(value: &MiValue) -> &[MiValue] {
        match value {
            MiValue::List(items) => items,
            other => panic!("expected a list, got {other:?}"),
        }
    }

    /// Run `parse` on its own thread and fail if it does not come back: the
    /// defect under test is a loop that never ends and allocates as it goes.
    fn parse_within_a_second(line: &'static str) -> Vec<MiResponse> {
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut parser = GdbMiParser::new();
            parser.feed(line);
            let _ = sender.send(parser.parse());
        });
        receiver
            .recv_timeout(std::time::Duration::from_secs(1))
            .unwrap_or_else(|_| panic!("parsing {line:?} did not finish"))
    }

    #[test]
    fn a_stray_closing_brace_inside_a_list_ends_the_parse() {
        // `}` closes nothing inside `[...]`. The value loop read an empty
        // constant there, consumed nothing, and read it again for ever.
        for line in [
            "^done,x=[}]\n",
            "^done,x=[\"a\"}]\n",
            "^done,x=[\"a\"},\"b\"]\n",
            "^done,x=[}}}]\n",
        ] {
            let responses = parse_within_a_second(line);
            assert_eq!(responses.len(), 1, "{line:?}");
        }
        // What is well formed around the stray delimiter is still read.
        let results = first_result("^done,x=[\"a\"},\"b\"],y=\"1\"\n");
        assert_eq!(
            list(&results["x"]).iter().map(constant).collect::<Vec<_>>(),
            ["a", "b"]
        );
        assert_eq!(constant(&results["y"]), "1");
    }

    #[test]
    fn a_list_inside_a_list_is_a_list() {
        // `[[a="1"]]` was handed to the result parser because it contains an
        // `=`, which produced a tuple with the key `[a`.
        let results = first_result("^done,x=[[a=\"1\"],[\"p\",\"q\"]]\n");
        let outer = list(&results["x"]);
        assert_eq!(outer.len(), 2);
        let first = list(&outer[0]);
        assert_eq!(first.len(), 1);
        assert_eq!(constant(&tuple(&first[0])["a"]), "1");
        assert_eq!(
            list(&outer[1]).iter().map(constant).collect::<Vec<_>>(),
            ["p", "q"]
        );
    }

    #[test]
    fn a_result_list_keeps_every_entry_of_a_repeated_key() {
        // The shape of `-stack-list-frames`: every entry has the key `frame`.
        // One map for the list kept the last frame only.
        let results = first_result(
            "^done,stack=[frame={level=\"0\",func=\"inner\"},frame={level=\"1\",func=\"main\"}]\n",
        );
        let frames = list(&results["stack"]);
        assert_eq!(frames.len(), 2);
        let funcs: Vec<&str> = frames
            .iter()
            .map(|entry| constant(&tuple(&tuple(entry)["frame"])["func"]))
            .collect();
        assert_eq!(funcs, ["inner", "main"]);
    }

    #[test]
    fn a_list_of_tuples_is_unchanged() {
        // Positive control: the shape `-data-list-register-values` returns,
        // which `RRDebugger::parse_registers` reads, is what it was.
        let results = first_result(
            "^done,register-values=[{number=\"0\",value=\"0x1\"},{number=\"16\",value=\"0x401000\"}]\n",
        );
        let registers = list(&results["register-values"]);
        assert_eq!(registers.len(), 2);
        assert_eq!(constant(&tuple(&registers[1])["number"]), "16");
        assert_eq!(constant(&tuple(&registers[1])["value"]), "0x401000");
        // And an empty list is empty.
        assert!(list(&first_result("^done,x=[]\n")["x"]).is_empty());
    }

    fn stream(line: &str) -> String {
        let mut parser = GdbMiParser::new();
        parser.feed(line);
        match parser.parse().into_iter().next() {
            Some(
                MiResponse::ConsoleStream(text)
                | MiResponse::TargetStream(text)
                | MiResponse::LogStream(text),
            ) => text,
            other => panic!("expected a stream record, got {other:?}"),
        }
    }

    #[test]
    fn a_stream_that_ends_in_an_escaped_quote_keeps_it() {
        // `~"say \"hi\""`: the text ends in a quote. Trimming every quote from
        // the end also took that one, leaving `say "hi\`.
        assert_eq!(stream("~\"say \\\"hi\\\"\"\n"), "say \"hi\"");
        assert_eq!(stream("&\"\\\"quoted\\\"\\n\"\n"), "\"quoted\"\n");
        assert_eq!(stream("@\"\\\"\"\n"), "\"");
    }

    #[test]
    fn a_plain_stream_is_unchanged() {
        // Positive control for the test above.
        assert_eq!(stream("~\"hello\\n\"\n"), "hello\n");
        assert_eq!(stream("~\"\"\n"), "");
    }
}
