//! YAML frontmatter parse / stringify, byte-compatible with server/utils/frontmatter.ts
//! (`yaml` package: core schema, `lineWidth: 0`, plain strings, block sequences indented under their key).
//!
//! Parsing uses `serde_yaml_ng` (maintained fork; `serde_yaml` is archived). Emitting is hand-written so that
//! quoting / layout match the previous implementation and re-saving an existing record produces no diff.

use super::error::{AppError, ErrorCode, Result};
use serde_json::{Map, Value};

pub type Data = Map<String, Value>;

#[derive(Debug)]
pub struct Parsed {
    pub data: Data,
    pub body: String,
}

/// Split `--- yaml ---` frontmatter from the body. Missing frontmatter yields empty data and the whole text as body.
pub fn parse(raw: &str) -> Result<Parsed> {
    let text = raw.strip_prefix('\u{feff}').unwrap_or(raw);
    let Some(rest) = text.strip_prefix("---\r\n").or_else(|| text.strip_prefix("---\n")) else {
        return Ok(Parsed { data: Data::new(), body: text.to_string() });
    };
    let (yaml, after): (&str, &str) = if let Some(after) = rest.strip_prefix("---") {
        ("", after)
    } else if let Some(i) = rest.find("\n---") {
        let y = &rest[..i];
        (y.strip_suffix('\r').unwrap_or(y), &rest[i + 4..])
    } else {
        return Ok(Parsed { data: Data::new(), body: text.to_string() });
    };
    let body = after.strip_prefix("\r\n").or_else(|| after.strip_prefix('\n')).unwrap_or(after);
    let value: serde_yaml_ng::Value = if yaml.trim().is_empty() {
        serde_yaml_ng::Value::Null
    } else {
        serde_yaml_ng::from_str(yaml).map_err(|e| {
            AppError::new(ErrorCode::InvalidFrontmatter, "YAML frontmatter を解析できません").with_detail(e.to_string())
        })?
    };
    let data = match yaml_to_json(value) {
        Value::Null => Data::new(),
        Value::Object(m) => m,
        _ => return Err(AppError::new(ErrorCode::InvalidFrontmatter, "frontmatter は YAML のマッピングである必要があります")),
    };
    Ok(Parsed { data, body: body.to_string() })
}

fn yaml_to_json(v: serde_yaml_ng::Value) -> Value {
    use serde_yaml_ng::Value as Y;
    match v {
        Y::Null => Value::Null,
        Y::Bool(b) => Value::Bool(b),
        Y::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::from(i)
            } else if let Some(u) = n.as_u64() {
                Value::from(u)
            } else {
                n.as_f64().and_then(serde_json::Number::from_f64).map(Value::Number).unwrap_or(Value::Null)
            }
        }
        Y::String(s) => Value::String(s),
        Y::Sequence(s) => Value::Array(s.into_iter().map(yaml_to_json).collect()),
        Y::Mapping(m) => {
            let mut out = Map::new();
            for (k, v) in m {
                let key = match k {
                    Y::String(s) => s,
                    Y::Bool(b) => b.to_string(),
                    Y::Number(n) => n.to_string(),
                    Y::Null => "null".to_string(),
                    other => serde_yaml_ng::to_string(&other).unwrap_or_default().trim().to_string(),
                };
                out.insert(key, yaml_to_json(v));
            }
            Value::Object(out)
        }
        Y::Tagged(t) => yaml_to_json(t.value),
    }
}

/// Serialise frontmatter + body: `---\n<yaml>\n---\n\n<body>\n`. Null-valued keys are kept (`key: null`).
pub fn stringify(data: &Data, body: &str) -> String {
    let mut yaml = String::new();
    emit_map(data, 0, &mut yaml);
    // The old `yaml` package writes an empty mapping as `{}`.
    let yaml = if data.is_empty() { "{}" } else { yaml.trim_end() };
    let trimmed = strip_leading_blank(body);
    // `trimmedBody.endsWith('\n') ? trimmedBody : trimmedBody + '\n'` (so an empty body still gets one newline)
    let nl = if trimmed.ends_with('\n') { "" } else { "\n" };
    format!("---\n{yaml}\n---\n\n{trimmed}{nl}")
}

/// `body.replace(/^\s*\n/, '')`
fn strip_leading_blank(body: &str) -> &str {
    let ws_end = body.find(|c: char| !c.is_whitespace()).unwrap_or(body.len());
    match body[..ws_end].rfind('\n') {
        Some(i) => &body[i + 1..],
        None => body,
    }
}

fn spaces(n: usize) -> String {
    " ".repeat(n)
}

fn emit_map(m: &Data, indent: usize, out: &mut String) {
    for (k, v) in m {
        out.push_str(&spaces(indent));
        out.push_str(&emit_string(k, true, indent));
        out.push(':');
        emit_value_after_key(v, indent, out);
    }
}

fn emit_value_after_key(v: &Value, indent: usize, out: &mut String) {
    match v {
        Value::Object(m) if !m.is_empty() => {
            out.push('\n');
            emit_map(m, indent + 2, out);
        }
        Value::Array(a) if !a.is_empty() => {
            out.push('\n');
            emit_seq(a, indent + 2, out);
        }
        Value::String(s) if s.contains('\n') && block_ok(s) => {
            out.push(' ');
            out.push_str(&block_string(s, indent + 2));
            out.push('\n');
        }
        other => {
            out.push(' ');
            out.push_str(&emit_inline(other, indent + 2));
            out.push('\n');
        }
    }
}

fn emit_seq(a: &[Value], indent: usize, out: &mut String) {
    for item in a {
        out.push_str(&spaces(indent));
        out.push('-');
        match item {
            Value::Object(m) if !m.is_empty() => {
                let mut sub = String::new();
                emit_map(m, indent + 2, &mut sub);
                out.push(' ');
                out.push_str(&sub[indent + 2..]);
            }
            Value::Array(inner) if !inner.is_empty() => {
                let mut sub = String::new();
                emit_seq(inner, indent + 2, &mut sub);
                out.push(' ');
                out.push_str(&sub[indent + 2..]);
            }
            Value::String(s) if s.contains('\n') && block_ok(s) => {
                out.push(' ');
                out.push_str(&block_string(s, indent + 2));
                out.push('\n');
            }
            other => {
                out.push(' ');
                out.push_str(&emit_inline(other, indent + 2));
                out.push('\n');
            }
        }
    }
}

/// Scalars and empty collections.
fn emit_inline(v: &Value, indent: usize) -> String {
    match v {
        Value::Null => "null".into(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => emit_string(s, false, indent),
        Value::Array(_) => "[]".into(),
        Value::Object(_) => "{}".into(),
    }
}

fn has_control_chars(s: &str) -> bool {
    s.chars().any(|c| matches!(c, '\u{0}'..='\u{8}' | '\u{b}'..='\u{1f}' | '\u{7f}'..='\u{9f}'))
}

/// A multi-line value that can be a literal block scalar (otherwise it is double-quoted).
fn block_ok(s: &str) -> bool {
    if has_control_chars(s) {
        return false;
    }
    // yaml: /\n[\t ]+$/ or all-whitespace -> quoted
    if s.trim().is_empty() {
        return false;
    }
    let tail_ws = s.rfind('\n').map(|i| &s[i + 1..]).unwrap_or("");
    !(!tail_ws.is_empty() && tail_ws.chars().all(|c| c == ' ' || c == '\t'))
}

fn emit_string(s: &str, implicit_key: bool, indent: usize) -> String {
    if has_control_chars(s) {
        return double_quoted(s);
    }
    if s.is_empty() {
        return "\"\"".into();
    }
    if implicit_key && s.contains('\n') {
        return quoted(s);
    }
    if needs_quote_by_syntax(s) || resolves_to_non_string(s) {
        // multi-line values that need quoting are emitted as block scalars (handled by the caller when possible)
        return quoted(s);
    }
    let _ = indent;
    s.to_string()
}

fn quoted(s: &str) -> String {
    let has_double = s.contains('"');
    let has_single = s.contains('\'');
    if has_double && !has_single {
        single_quoted(s)
    } else {
        double_quoted(s)
    }
}

fn single_quoted(s: &str) -> String {
    if s.contains('\n') {
        return double_quoted(s);
    }
    format!("'{}'", s.replace('\'', "''"))
}

fn double_quoted(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\0' => out.push_str("\\0"),
            c if (c as u32) < 0x100 && (c.is_control()) => out.push_str(&format!("\\x{:02X}", c as u32)),
            c if c.is_control() => out.push_str(&format!("\\u{:04X}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// /^[\n\t ,[\]{}#&*!|>'"%@`]|^[?-]$|^[?-][ \t]|[\n:][ \t]|[ \t]\n|[\n\t ]#|[\n\t :]$/
fn needs_quote_by_syntax(s: &str) -> bool {
    let chars: Vec<char> = s.chars().collect();
    if matches!(chars[0], '\n' | '\t' | ' ' | ',' | '[' | ']' | '{' | '}' | '#' | '&' | '*' | '!' | '|' | '>' | '\'' | '"' | '%' | '@' | '`') {
        return true;
    }
    if (chars[0] == '?' || chars[0] == '-') && (chars.len() == 1 || chars[1] == ' ' || chars[1] == '\t') {
        return true;
    }
    for w in chars.windows(2) {
        let (a, b) = (w[0], w[1]);
        if (a == '\n' || a == ':') && (b == ' ' || b == '\t') {
            return true;
        }
        if (a == ' ' || a == '\t') && b == '\n' {
            return true;
        }
        if (a == '\n' || a == '\t' || a == ' ') && b == '#' {
            return true;
        }
    }
    matches!(chars[chars.len() - 1], '\n' | '\t' | ' ' | ':')
}

/// Strings that the YAML 1.2 core schema would read back as null / bool / int / float.
fn resolves_to_non_string(s: &str) -> bool {
    if matches!(s, "" | "~" | "null" | "Null" | "NULL" | "true" | "True" | "TRUE" | "false" | "False" | "FALSE") {
        return true;
    }
    if matches!(s, ".nan" | ".NaN" | ".NAN") {
        return true;
    }
    let unsigned = s.strip_prefix(['-', '+']).unwrap_or(s);
    if matches!(unsigned, ".inf" | ".Inf" | ".INF") {
        return true;
    }
    if let Some(h) = s.strip_prefix("0x") {
        if !h.is_empty() && h.chars().all(|c| c.is_ascii_hexdigit()) {
            return true;
        }
    }
    if let Some(o) = s.strip_prefix("0o") {
        if !o.is_empty() && o.chars().all(|c| ('0'..='7').contains(&c)) {
            return true;
        }
    }
    is_core_float_or_int(unsigned)
}

/// `(\.[0-9]+|[0-9]+(\.[0-9]*)?)([eE][-+]?[0-9]+)?` (also covers plain decimal ints)
fn is_core_float_or_int(s: &str) -> bool {
    let b = s.as_bytes();
    let mut i = 0;
    let int_start = i;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    let int_digits = i - int_start;
    if int_digits > 0 {
        if i < b.len() && b[i] == b'.' {
            i += 1;
            while i < b.len() && b[i].is_ascii_digit() {
                i += 1;
            }
        }
    } else if i < b.len() && b[i] == b'.' {
        i += 1;
        let frac_start = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        if i == frac_start {
            return false;
        }
    } else {
        return false;
    }
    if i < b.len() && (b[i] == b'e' || b[i] == b'E') {
        i += 1;
        if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
            i += 1;
        }
        let exp_start = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        if i == exp_start {
            return false;
        }
    }
    i == b.len()
}

/// Append `pad` after every run of newlines in `s` (JS `s.replace(/\n+/g, `$&${pad}`)`). With `skip_trailing` a run at the
/// very end of `s` is left alone (`/\n+(?!\n|$)/g`).
fn pad_newline_runs(s: &str, pad: &str, skip_trailing: bool) -> String {
    let mut out = String::with_capacity(s.len() + pad.len());
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        out.push(chars[i]);
        if chars[i] == '\n' && chars.get(i + 1) != Some(&'\n') && !(skip_trailing && i + 1 == chars.len()) {
            out.push_str(pad);
        }
        i += 1;
    }
    out
}

/// Literal block scalar (`|`, `|-`, `|+`) for a multi-line string; a port of `blockString` of the `yaml` package.
/// `indent` is the content indentation. Trailing spaces / tabs of the last line and leading blank lines are kept.
fn block_string(value: &str, indent: usize) -> String {
    let pad = spaces(indent);
    let end_start = value.trim_end_matches(['\n', '\t', ' ']).len();
    let mut end = value[end_start..].to_string();
    let chomp = match end.find('\n') {
        None => "-",
        Some(pos) if value.len() == end.len() || pos != end.len() - 1 => "+",
        Some(_) => "",
    };
    let body = &value[..end_start];
    if end.ends_with('\n') {
        end.pop();
    }
    let end = pad_newline_runs(&end, &pad, true);
    // leading blank lines / spaces
    let mut start_with_space = false;
    let mut start_nl: Option<usize> = None;
    for (i, c) in body.char_indices() {
        match c {
            ' ' => start_with_space = true,
            '\n' => start_nl = Some(i),
            _ => break,
        }
    }
    let (start, rest) = match start_nl {
        Some(i) => body.split_at(i + 1),
        None => ("", body),
    };
    let start = pad_newline_runs(start, &pad, false);
    let rest = pad_newline_runs(rest, &pad, false);
    format!("|{}{}\n{}{}{}{}", if start_with_space { "2" } else { "" }, chomp, pad, start, rest, end)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn obj(v: Value) -> Data {
        v.as_object().unwrap().clone()
    }

    #[test]
    fn round_trips_thai_japanese_and_arrays() {
        let data = obj(json!({
            "slug": "somdet-to", "kind": "monks", "name": "ソムデット・トー",
            "name_th": "สมเด็จพระพุฒาจารย์ (โต พฺรหฺมรํสี)",
            "aliases": ["ルアンポー・トー", "หลวงพ่อโต"], "tags": []
        }));
        let body = "# ソムデット・トー\n\n## 概要\nワット・ラカンの住職。\n";
        let raw = stringify(&data, body);
        assert!(raw.starts_with("---\n"));
        assert!(raw.contains("สมเด็จพระพุฒาจารย์"));
        assert!(raw.contains("tags: []"));
        let p = parse(&raw).unwrap();
        assert_eq!(p.data, data);
        assert_eq!(p.body.trim_start_matches('\n'), body);
        assert_eq!(stringify(&p.data, &p.body), raw, "re-serialising is stable");
    }

    #[test]
    fn key_order_is_preserved() {
        let raw = "---\nz: 1\na: 2\nm: 3\n---\nbody\n";
        let p = parse(raw).unwrap();
        let keys: Vec<_> = p.data.keys().cloned().collect();
        assert_eq!(keys, ["z", "a", "m"]);
    }

    #[test]
    fn quotes_colons_and_numeric_looking_strings() {
        let data = obj(json!({ "born": "1788", "name": "Wat: Rakhang", "url": "https://example.com/a?b=c", "t": "10:30", "flag": "true", "e": "", "n": "null", "f": "1.5e3", "hex": "0x1F" }));
        let raw = stringify(&data, "x");
        assert!(raw.contains("born: \"1788\"\n"), "{raw}");
        assert!(raw.contains("name: \"Wat: Rakhang\"\n"), "{raw}");
        assert!(raw.contains("url: https://example.com/a?b=c\n"), "{raw}");
        assert!(raw.contains("t: \"10:30\"\n") || raw.contains("t: 10:30\n"), "{raw}");
        assert!(raw.contains("flag: \"true\"\n"));
        assert!(raw.contains("e: \"\"\n"));
        assert!(raw.contains("n: \"null\"\n"));
        assert!(raw.contains("f: \"1.5e3\"\n"));
        assert!(raw.contains("hex: \"0x1F\"\n"));
        assert_eq!(parse(&raw).unwrap().data, data);
    }

    #[test]
    fn quote_style_selection() {
        let data = obj(json!({ "a": "say \"hi\"", "b": "it's", "c": "both ' and \"", "d": "- x", "e": "# c", "f": "tail:" }));
        let raw = stringify(&data, "");
        // Same as the TS `yaml` lib (defaultStringType PLAIN): plain unless the scalar is ambiguous.
        assert!(raw.contains("a: say \"hi\"\n"), "{raw}");
        assert!(raw.contains("b: it's\n"), "{raw}");
        assert!(raw.contains("c: both ' and \"\n"), "{raw}");
        assert!(raw.contains("d: \"- x\"\n"));
        assert!(raw.contains("e: \"# c\"\n"));
        assert!(raw.contains("f: \"tail:\"\n"));
        assert_eq!(parse(&raw).unwrap().data, data);
    }

    #[test]
    fn numbers_bools_nulls_and_nested() {
        let data = obj(json!({ "n": 5, "f": 1.5, "b": true, "z": null, "m": { "x": [1, { "k": "v", "l": [] }], "e": {} } }));
        let raw = stringify(&data, "");
        assert!(raw.contains("n: 5\n") && raw.contains("f: 1.5\n") && raw.contains("b: true\n") && raw.contains("z: null\n"));
        assert!(raw.contains("m:\n  x:\n    - 1\n    - k: v\n      l: []\n  e: {}\n"), "{raw}");
        assert_eq!(parse(&raw).unwrap().data, data);
    }

    #[test]
    fn multiline_string_is_literal_block() {
        let data = obj(json!({ "kept": "a\nb\n", "note": "line1\nline2" }));
        let raw = stringify(&data, "");
        assert!(raw.contains("kept: |\n  a\n  b\nnote: |-\n  line1\n  line2\n"), "{raw}");
        assert_eq!(parse(&raw).unwrap().data, data);
    }

    #[test]
    fn block_scalars_keep_trailing_spaces_and_do_not_grow_on_resave() {
        // trailing space on the last line, `|+` keep-chomp in the middle, leading blank line (found by diffing against the `yaml` package)
        let data = obj(json!({ "a": "x\nfoo ", "keep": "x\n\n", "lead": "\nlead", "z": "end" }));
        let raw = stringify(&data, "b");
        assert_eq!(parse(&raw).unwrap().data, data);
        let again = stringify(&parse(&raw).unwrap().data, &parse(&raw).unwrap().body);
        assert_eq!(raw, again);
        assert!(raw.contains("a: |-\n  x\n  foo \n"), "{raw}");
        assert!(raw.contains("keep: |+\n  x\n\nlead:"), "{raw}");
        assert!(raw.contains("lead: |-\n  \n  lead\n"), "{raw}");
    }

    #[test]
    fn document_marker_lookalikes_stay_plain_like_the_old_output() {
        let data = obj(json!({ "a": "---", "b": "...and so on", "c": "--- x" }));
        let raw = stringify(&data, "b");
        assert!(raw.contains("a: ---\nb: ...and so on\nc: --- x\n"), "{raw}");
        assert_eq!(parse(&raw).unwrap().data, data);
    }

    #[test]
    fn empty_data_and_empty_body_match_the_old_output() {
        assert_eq!(stringify(&Data::new(), "only"), "---\n{}\n---\n\nonly\n");
        assert_eq!(stringify(&obj(json!({ "n": "x" })), ""), "---\nn: x\n---\n\n\n");
    }

    #[test]
    fn unquoted_dates_stay_strings() {
        let p = parse("---\nborn: 1788-01-01\ncreated_at: 2026-11-05T00:00:00.000Z\n---\nbody\n").unwrap();
        assert_eq!(p.data["born"], "1788-01-01");
        assert_eq!(p.data["created_at"], "2026-11-05T00:00:00.000Z");
        let raw = stringify(&p.data, "");
        assert!(raw.contains("created_at: 2026-11-05T00:00:00.000Z\n"));
    }

    #[test]
    fn no_frontmatter_and_bom_crlf() {
        let p = parse("# タイトル\n本文").unwrap();
        assert!(p.data.is_empty());
        assert_eq!(p.body, "# タイトル\n本文");
        let p = parse("\u{feff}---\r\nname: テスト\r\n---\r\nbody").unwrap();
        assert_eq!(p.data["name"], "テスト");
        assert_eq!(p.body, "body");
    }

    #[test]
    fn invalid_yaml_and_non_mapping() {
        assert_eq!(parse("---\nfoo: [unclosed\n---\nb").unwrap_err().code, ErrorCode::InvalidFrontmatter);
        assert_eq!(parse("---\n- a\n- b\n---\nb").unwrap_err().code, ErrorCode::InvalidFrontmatter);
    }

    #[test]
    fn empty_frontmatter() {
        let p = parse("---\n---\nbody\n").unwrap();
        assert!(p.data.is_empty());
        assert_eq!(p.body, "body\n");
    }

    #[test]
    fn leading_blank_lines_of_body_are_dropped_once() {
        let raw = stringify(&obj(json!({ "a": 1 })), "\n\n# T\n");
        assert_eq!(raw, "---\na: 1\n---\n\n# T\n");
        let raw = stringify(&obj(json!({ "a": 1 })), "no newline");
        assert!(raw.ends_with("no newline\n"));
    }
}
