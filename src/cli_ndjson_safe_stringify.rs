// Source: ~/claudecode/openclaudecode/src/cli/ndjsonSafeStringify.ts
//! NDJSON-safe JSON serialization.
//!
//! JSON.stringify emits U+2028/U+2029 raw (valid per ECMA-404). When the
//! output is a single NDJSON line, any receiver that uses JavaScript
//! line-terminator semantics (ECMA-262 §11.3 — \n \r U+2028 U+2029) to
//! split the stream will cut the JSON mid-string.
//!
//! The \uXXXX form is equivalent JSON but can never be mistaken for a
//! line terminator by ANY receiver.

#![allow(dead_code)]

/// Escape U+2028 (LINE SEPARATOR) and U+2029 (PARAGRAPH SEPARATOR) in JSON output.
///
/// These characters are valid in JSON strings per ECMA-404 but treated as
/// line terminators in JavaScript (ECMA-262). When NDJSON output is consumed
/// by JS code that splits on line terminators, unescaped U+2028/U+2029 will
/// silently break the stream.
fn escape_js_line_terminators(json: &str) -> String {
    json.replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029")
}

/// Serialize any serde value to NDJSON-safe JSON.
pub fn serialize_to_ndjson<T: serde::Serialize>(value: &T) -> Result<String, serde_json::Error> {
    let json = serde_json::to_string(value)?;
    Ok(escape_js_line_terminators(&json))
}

/// Serialize a JSON value to NDJSON-safe string, falling back to empty string on error.
pub fn serialize_to_ndjson_safe(value: &serde_json::Value) -> String {
    let json = serde_json::to_string(value).unwrap_or_default();
    escape_js_line_terminators(&json)
}

/// Escape U+2028/U+2029 in an arbitrary string (not just JSON).
/// Useful for session transcript content that may contain these characters.
pub fn escape_unicode_line_separators(text: &str) -> String {
    escape_js_line_terminators(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_escape_u2028() {
        let json = r#""hello world""#;
        // The actual U+2028 character in a string
        let input = "hello\u{2028}world";
        let quoted = format!("\"{}\"", input);
        let escaped = escape_js_line_terminators(&quoted);
        assert!(escaped.contains("\\u2028"));
        assert!(!escaped.contains('\u{2028}'));
    }

    #[test]
    fn test_escape_u2029() {
        let input = "hello\u{2029}world";
        let quoted = format!("\"{}\"", input);
        let escaped = escape_js_line_terminators(&quoted);
        assert!(escaped.contains("\\u2029"));
        assert!(!escaped.contains('\u{2029}'));
    }

    #[test]
    fn test_escape_both() {
        let input = "\u{2028}start\u{2029}middle\u{2028}end";
        let escaped = escape_js_line_terminators(input);
        assert_eq!(escaped, "\\u2028start\\u2029middle\\u2028end");
    }

    #[test]
    fn test_no_escape_needed() {
        let input = r#""hello world""#;
        let escaped = escape_js_line_terminators(input);
        assert_eq!(escaped, r#""hello world""#);
    }

    #[test]
    fn test_serialize_to_ndjson_safe() {
        let value = serde_json::json!("test\u{2028}value\u{2029}end");
        let result = serialize_to_ndjson_safe(&value);
        assert!(result.contains("\\u2028"));
        assert!(result.contains("\\u2029"));
        // Result must be valid JSON
        assert!(serde_json::from_str::<serde_json::Value>(&result).is_ok());
    }

    #[test]
    fn test_serialize_to_ndjson_with_struct() {
        #[derive(serde::Serialize)]
        struct Test {
            text: String,
        }
        let t = Test {
            text: "line\u{2028}separator".to_string(),
        };
        let result = serialize_to_ndjson(&t).unwrap();
        assert!(result.contains("\\u2028"));
    }

    #[test]
    fn test_escape_unicode_line_separators() {
        let input = "normal\nline\u{2028}separator\u{2029}paragraph";
        let escaped = escape_unicode_line_separators(&input);
        // Regular newlines should be preserved
        assert!(escaped.contains('\n'));
        // U+2028/U+2029 should be escaped
        assert!(escaped.contains("\\u2028"));
        assert!(escaped.contains("\\u2029"));
    }

    #[test]
    fn test_roundtrip_parsing() {
        // Verify escaped JSON parses to the same value
        let original = "hello\u{2028}world\u{2029}!";
        let value = serde_json::json!(original);
        let escaped = serialize_to_ndjson_safe(&value);
        let parsed = serde_json::from_str::<serde_json::Value>(&escaped).unwrap();
        assert_eq!(parsed.as_str().unwrap(), original);
    }
}
