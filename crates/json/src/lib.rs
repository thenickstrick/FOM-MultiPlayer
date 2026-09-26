//! Hand-rolled generic JSON value type, parser, and writer. See
//! `README.md` for why (no serde) and the opaque-round-trip design.

use std::fmt::Write as _;

#[derive(Debug, Clone, PartialEq)]
pub enum JsonValue {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<JsonValue>),
    Object(Vec<(String, JsonValue)>),
}

impl JsonValue {
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            JsonValue::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_number(&self) -> Option<f64> {
        match self {
            JsonValue::Number(n) => Some(*n),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            JsonValue::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&[JsonValue]> {
        match self {
            JsonValue::Array(a) => Some(a),
            _ => None,
        }
    }

    /// Looks up a key in an object, first match wins. Returns `None` for
    /// non-objects or missing keys.
    pub fn get(&self, key: &str) -> Option<&JsonValue> {
        match self {
            JsonValue::Object(entries) => entries.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub message: String,
    pub position: usize,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "JSON parse error at byte {}: {}", self.position, self.message)
    }
}

impl std::error::Error for ParseError {}

pub fn parse(input: &str) -> Result<JsonValue, ParseError> {
    let mut parser = Parser {
        bytes: input.as_bytes(),
        pos: 0,
    };
    parser.skip_whitespace();
    let value = parser.parse_value()?;
    parser.skip_whitespace();
    if parser.pos != parser.bytes.len() {
        return Err(parser.error("trailing data after JSON value"));
    }
    Ok(value)
}

struct Parser<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Parser<'a> {
    fn error(&self, message: &str) -> ParseError {
        ParseError {
            message: message.to_string(),
            position: self.pos,
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn advance(&mut self) -> Option<u8> {
        let b = self.peek();
        if b.is_some() {
            self.pos += 1;
        }
        b
    }

    fn skip_whitespace(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.pos += 1;
        }
    }

    fn expect_byte(&mut self, expected: u8) -> Result<(), ParseError> {
        match self.advance() {
            Some(b) if b == expected => Ok(()),
            Some(b) => Err(self.error(&format!(
                "expected '{}', found '{}'",
                expected as char, b as char
            ))),
            None => Err(self.error(&format!("expected '{}', found end of input", expected as char))),
        }
    }

    fn expect_literal(&mut self, literal: &str) -> Result<(), ParseError> {
        for expected in literal.bytes() {
            self.expect_byte(expected)?;
        }
        Ok(())
    }

    fn parse_value(&mut self) -> Result<JsonValue, ParseError> {
        self.skip_whitespace();
        match self.peek() {
            Some(b'{') => self.parse_object(),
            Some(b'[') => self.parse_array(),
            Some(b'"') => self.parse_string().map(JsonValue::String),
            Some(b't') => {
                self.expect_literal("true")?;
                Ok(JsonValue::Bool(true))
            }
            Some(b'f') => {
                self.expect_literal("false")?;
                Ok(JsonValue::Bool(false))
            }
            Some(b'n') => {
                self.expect_literal("null")?;
                Ok(JsonValue::Null)
            }
            Some(b'-' | b'0'..=b'9') => self.parse_number(),
            Some(other) => Err(self.error(&format!("unexpected character '{}'", other as char))),
            None => Err(self.error("unexpected end of input")),
        }
    }

    fn parse_object(&mut self) -> Result<JsonValue, ParseError> {
        self.expect_byte(b'{')?;
        let mut entries = Vec::new();
        self.skip_whitespace();
        if self.peek() == Some(b'}') {
            self.pos += 1;
            return Ok(JsonValue::Object(entries));
        }
        loop {
            self.skip_whitespace();
            if self.peek() != Some(b'"') {
                return Err(self.error("expected string key"));
            }
            let key = self.parse_string()?;
            self.skip_whitespace();
            self.expect_byte(b':')?;
            let value = self.parse_value()?;
            entries.push((key, value));
            self.skip_whitespace();
            match self.advance() {
                Some(b',') => continue,
                Some(b'}') => break,
                Some(other) => return Err(self.error(&format!("expected ',' or '}}', found '{}'", other as char))),
                None => return Err(self.error("unexpected end of input in object")),
            }
        }
        Ok(JsonValue::Object(entries))
    }

    fn parse_array(&mut self) -> Result<JsonValue, ParseError> {
        self.expect_byte(b'[')?;
        let mut items = Vec::new();
        self.skip_whitespace();
        if self.peek() == Some(b']') {
            self.pos += 1;
            return Ok(JsonValue::Array(items));
        }
        loop {
            let value = self.parse_value()?;
            items.push(value);
            self.skip_whitespace();
            match self.advance() {
                Some(b',') => continue,
                Some(b']') => break,
                Some(other) => return Err(self.error(&format!("expected ',' or ']', found '{}'", other as char))),
                None => return Err(self.error("unexpected end of input in array")),
            }
        }
        Ok(JsonValue::Array(items))
    }

    fn parse_string(&mut self) -> Result<String, ParseError> {
        self.expect_byte(b'"')?;
        let mut out = String::new();
        loop {
            match self.advance() {
                Some(b'"') => break,
                Some(b'\\') => match self.advance() {
                    Some(b'"') => out.push('"'),
                    Some(b'\\') => out.push('\\'),
                    Some(b'/') => out.push('/'),
                    Some(b'b') => out.push('\u{8}'),
                    Some(b'f') => out.push('\u{c}'),
                    Some(b'n') => out.push('\n'),
                    Some(b'r') => out.push('\r'),
                    Some(b't') => out.push('\t'),
                    Some(b'u') => {
                        let cp = self.parse_hex4()?;
                        if (0xD800..=0xDBFF).contains(&cp) {
                            self.expect_byte(b'\\')?;
                            self.expect_byte(b'u')?;
                            let low = self.parse_hex4()?;
                            if !(0xDC00..=0xDFFF).contains(&low) {
                                return Err(self.error("invalid low surrogate"));
                            }
                            let c = 0x10000 + ((cp - 0xD800) << 10) + (low - 0xDC00);
                            out.push(char::from_u32(c).ok_or_else(|| self.error("invalid surrogate pair"))?);
                        } else {
                            out.push(char::from_u32(cp).ok_or_else(|| self.error("invalid \\u escape"))?);
                        }
                    }
                    Some(other) => return Err(self.error(&format!("invalid escape '\\{}'", other as char))),
                    None => return Err(self.error("unexpected end of input in escape")),
                },
                Some(b) if b < 0x80 => out.push(b as char),
                Some(b) => {
                    // Multi-byte UTF-8 sequence: consume the remaining
                    // continuation bytes and decode the whole run.
                    let start = self.pos - 1;
                    let extra = if b >= 0xF0 {
                        3
                    } else if b >= 0xE0 {
                        2
                    } else {
                        1
                    };
                    for _ in 0..extra {
                        self.advance().ok_or_else(|| self.error("truncated UTF-8 sequence"))?;
                    }
                    let slice = &self.bytes[start..self.pos];
                    let s = std::str::from_utf8(slice).map_err(|_| self.error("invalid UTF-8 sequence"))?;
                    out.push_str(s);
                }
                None => return Err(self.error("unterminated string")),
            }
        }
        Ok(out)
    }

    fn parse_hex4(&mut self) -> Result<u32, ParseError> {
        let mut value: u32 = 0;
        for _ in 0..4 {
            let b = self.advance().ok_or_else(|| self.error("truncated \\u escape"))?;
            let digit = match b {
                b'0'..=b'9' => b - b'0',
                b'a'..=b'f' => b - b'a' + 10,
                b'A'..=b'F' => b - b'A' + 10,
                _ => return Err(self.error("invalid hex digit in \\u escape")),
            };
            value = value * 16 + digit as u32;
        }
        Ok(value)
    }

    fn parse_number(&mut self) -> Result<JsonValue, ParseError> {
        let start = self.pos;
        if self.peek() == Some(b'-') {
            self.pos += 1;
        }
        match self.peek() {
            Some(b'0') => self.pos += 1,
            Some(b'1'..=b'9') => {
                while matches!(self.peek(), Some(b'0'..=b'9')) {
                    self.pos += 1;
                }
            }
            _ => return Err(self.error("invalid number")),
        }
        if self.peek() == Some(b'.') {
            self.pos += 1;
            if !matches!(self.peek(), Some(b'0'..=b'9')) {
                return Err(self.error("invalid number: expected digit after '.'"));
            }
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.pos += 1;
            }
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.pos += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.pos += 1;
            }
            if !matches!(self.peek(), Some(b'0'..=b'9')) {
                return Err(self.error("invalid number: expected digit in exponent"));
            }
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.pos += 1;
            }
        }
        let text = std::str::from_utf8(&self.bytes[start..self.pos]).unwrap();
        text.parse::<f64>()
            .map(JsonValue::Number)
            .map_err(|_| self.error("invalid number"))
    }
}

pub fn write(value: &JsonValue) -> String {
    let mut out = String::new();
    write_value(value, &mut out);
    out
}

fn write_value(value: &JsonValue, out: &mut String) {
    match value {
        JsonValue::Null => out.push_str("null"),
        JsonValue::Bool(true) => out.push_str("true"),
        JsonValue::Bool(false) => out.push_str("false"),
        JsonValue::Number(n) => write_number(*n, out),
        JsonValue::String(s) => write_string(s, out),
        JsonValue::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_value(item, out);
            }
            out.push(']');
        }
        JsonValue::Object(entries) => {
            out.push('{');
            for (i, (key, val)) in entries.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_string(key, out);
                out.push(':');
                write_value(val, out);
            }
            out.push('}');
        }
    }
}

fn write_number(n: f64, out: &mut String) {
    if n.fract() == 0.0 && n.abs() < 1e15 {
        let _ = write!(out, "{}", n as i64);
    } else {
        let _ = write!(out, "{}", n);
    }
}

fn write_string(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_primitives() {
        assert_eq!(parse("null").unwrap(), JsonValue::Null);
        assert_eq!(parse("true").unwrap(), JsonValue::Bool(true));
        assert_eq!(parse("false").unwrap(), JsonValue::Bool(false));
        assert_eq!(parse("42").unwrap(), JsonValue::Number(42.0));
        assert_eq!(parse("-3.5").unwrap(), JsonValue::Number(-3.5));
        assert_eq!(parse("1e3").unwrap(), JsonValue::Number(1000.0));
        assert_eq!(parse("\"hi\"").unwrap(), JsonValue::String("hi".to_string()));
    }

    #[test]
    fn parses_whitespace_around_value() {
        assert_eq!(parse("  \n\t 42 \n").unwrap(), JsonValue::Number(42.0));
    }

    #[test]
    fn parses_nested_object_and_array() {
        let input = r#"{"pos":{"x":1.5,"y":-2},"tags":["a","b",null],"active":true}"#;
        let value = parse(input).unwrap();
        assert_eq!(
            value.get("pos").unwrap().get("x").unwrap().as_number(),
            Some(1.5)
        );
        assert_eq!(
            value.get("tags").unwrap().as_array().unwrap().len(),
            3
        );
        assert_eq!(value.get("active").unwrap().as_bool(), Some(true));
    }

    #[test]
    fn parses_string_escapes_and_unicode() {
        let value = parse(r#""line1\nline2\t\"quoted\"é""#).unwrap();
        assert_eq!(value.as_str(), Some("line1\nline2\t\"quoted\"\u{e9}"));
    }

    #[test]
    fn parses_surrogate_pair_escape() {
        // U+1F600 (grinning face) as a UTF-16 surrogate pair.
        let value = parse(r#""😀""#).unwrap();
        assert_eq!(value.as_str(), Some("\u{1F600}"));
    }

    #[test]
    fn round_trips_arbitrary_unknown_fields_opaquely() {
        // Fields this crate doesn't know the meaning of must survive
        // a round trip unchanged (see README.md).
        let input = r#"{"player_id":"p1","pos":{"x":10,"y":20.25},"inventory":[{"slot":0,"item":"axe"},{"slot":1,"item":null}],"custom_future_field":{"nested":[1,2,3]}}"#;
        let value = parse(input).unwrap();
        let output = write(&value);
        let reparsed = parse(&output).unwrap();
        assert_eq!(value, reparsed);
    }

    #[test]
    fn writer_escapes_special_characters() {
        let value = JsonValue::String("a\"b\\c\nd".to_string());
        assert_eq!(write(&value), r#""a\"b\\c\nd""#);
    }

    #[test]
    fn writer_emits_integers_without_decimal_point() {
        assert_eq!(write(&JsonValue::Number(42.0)), "42");
        assert_eq!(write(&JsonValue::Number(-7.0)), "-7");
        assert_eq!(write(&JsonValue::Number(1.5)), "1.5");
    }

    #[test]
    fn empty_object_and_array_round_trip() {
        assert_eq!(parse("{}").unwrap(), JsonValue::Object(vec![]));
        assert_eq!(parse("[]").unwrap(), JsonValue::Array(vec![]));
        assert_eq!(write(&parse("{}").unwrap()), "{}");
        assert_eq!(write(&parse("[]").unwrap()), "[]");
    }

    #[test]
    fn rejects_trailing_garbage() {
        assert!(parse("42 43").is_err());
    }

    #[test]
    fn rejects_malformed_input() {
        assert!(parse("{").is_err());
        assert!(parse("[1,]").is_err());
        assert!(parse("\"unterminated").is_err());
        assert!(parse("tru").is_err());
        assert!(parse("").is_err());
    }
}
