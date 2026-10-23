//! Comparing a value with a field body as mewl built with utf8proc.
//!
//! Both are normalized before the comparison: composed canonically
//! (NFC), and, ignoring case, folded by Unicode full case folding, so
//! that "Straße" matches "STRASSE" and precomposed and decomposed
//! spellings agree. When either is not valid UTF-8, or the value is
//! normalized to nothing, they are compared byte by byte, ignoring the
//! case of ASCII letters only.
//!
//! mewl uses utf8proc; this uses unicode-normalization and caseless,
//! which may follow another version of Unicode.

use std::str;

use caseless::default_case_fold_str;
use unicode_normalization::UnicodeNormalization;

use crate::bytes::{BytesExt, Case};

/// Whether `value` occurs in `text` (`field_match()` of mewl).
pub fn field_match(text: &[u8], value: &[u8], case_sensitive: bool) -> bool {
    if let (Ok(text), Ok(value)) = (str::from_utf8(text), str::from_utf8(value)) {
        let value = normalize(value, case_sensitive);
        if !value.is_empty() {
            return normalize(text, case_sensitive).contains(&value);
        }
    }
    let case = if case_sensitive { Case::Sensitive } else { Case::Ignore };
    text.contains_bytes(value, case)
}

/// NFC, after full case folding unless `case_sensitive`.
fn normalize(s: &str, case_sensitive: bool) -> String {
    if case_sensitive {
        s.nfc().collect()
    } else {
        default_case_fold_str(s).nfc().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::field_match;

    fn matches(text: &str, value: &str, case_sensitive: bool) -> bool {
        field_match(text.as_bytes(), value.as_bytes(), case_sensitive)
    }

    #[test]
    fn ignoring_case() {
        assert!(matches("Straße", "STRASSE", false));
        assert!(matches("STRASSE", "straße", false));
        assert!(matches("ПРИВЕТ мир", "привет", false));
        assert!(matches("日本語の件名", "日本語", false));
        /* precomposed and decomposed */
        assert!(matches("\u{c4}rger", "a\u{308}rger", false));
        /* width is not folded */
        assert!(!matches("ＡＢＣ", "abc", false));
    }

    #[test]
    fn case_sensitive() {
        assert!(matches("\u{c4}rger", "A\u{308}rger", true));
        assert!(!matches("Straße", "STRASSE", true));
        assert!(!matches("Hello", "hello", true));
    }

    #[test]
    fn invalid_utf8() {
        /* byte by byte, ignoring the case of ASCII letters only */
        assert!(field_match(b"\xffHello", b"hello", false));
        assert!(field_match(b"\xc3\x84rger", b"\xc3\x84R", false));
        assert!(!field_match(b"\xff\xc3\xa4", "Ä".as_bytes(), false));
        assert!(field_match(b"abc", b"", false));
    }
}
