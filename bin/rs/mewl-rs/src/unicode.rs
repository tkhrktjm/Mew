//! Comparing a value with a field body as mewl built with utf8proc.
//!
//! Both are normalized before the comparison: composed canonically
//! (NFC), and, ignoring case, folded by Unicode full case folding, so
//! that "Straße" matches "STRASSE" and precomposed and decomposed
//! spellings agree. When either is not valid UTF-8, or the value is
//! normalized to nothing, they are compared byte by byte, ignoring the
//! case of ASCII letters only.
//!
//! `mewl-rs` ignores more when it ignores case ([`Fold::Char`]): as mewl
//! built with `--enable-utf8proc-char-fold`, differences by
//! compatibility, such as width, and default ignorable code points
//! (NFKC_Casefold), and moreover the seven characters of JIS X 0208
//! that Windows maps to other code points, e.g. "～" (U+FF5E) is "〜"
//! (U+301C), so that a value matches whichever table a mailer used.
//!
//! mewl uses utf8proc; this uses unicode-normalization and caseless,
//! which may follow another version of Unicode.

use std::str;

use caseless::default_case_fold_str;
use unicode_normalization::UnicodeNormalization;

use crate::bytes::{BytesExt, Case};

/// What a comparison ignores.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fold {
    /// Nothing but the difference of NFC and NFD (`==` and `!==`).
    None,
    /// Case, by Unicode full case folding (`=` and `!=` of mewl).
    Case,
    /// Case, compatibility differences, default ignorable code points,
    /// and the code points of Windows for JIS X 0208 (`=` and `!=` of
    /// `mewl-rs`).
    Char,
}

/// Whether `value` occurs in `text` (`field_match()` of mewl).
pub fn field_match(text: &[u8], value: &[u8], fold: Fold) -> bool {
    if let (Ok(text), Ok(value)) = (str::from_utf8(text), str::from_utf8(value)) {
        let value = normalize(value, fold);
        if !value.is_empty() {
            return normalize(text, fold).contains(&value);
        }
    }
    let case = if fold == Fold::None { Case::Sensitive } else { Case::Ignore };
    text.contains_bytes(value, case)
}

fn normalize(s: &str, fold: Fold) -> String {
    match fold {
        Fold::None => s.nfc().collect(),
        Fold::Case => default_case_fold_str(s).nfc().collect(),
        Fold::Char => nfkc_casefold(&s.chars().map(jis_from_windows).collect::<String>()),
    }
}

/// NFKC_Casefold as utf8proc does it: each character is decomposed by
/// compatibility and case-folded, recursively, and stripped if default
/// ignorable, before the canonical ordering and composition.
fn nfkc_casefold(s: &str) -> String {
    let mut decomposed = String::with_capacity(s.len());
    for c in s.chars() {
        decompose(c, &mut decomposed);
    }
    decomposed.nfkc().collect()
}

/// `c` decomposed as `utf8proc_decompose_char()` does with
/// `UTF8PROC_COMPAT`, `UTF8PROC_CASEFOLD`, and `UTF8PROC_IGNORE`.
fn decompose(c: char, out: &mut String) {
    if is_ignorable(c) {
        return;
    }
    let folded = default_case_fold_str(c.encode_utf8(&mut [0; 4]));
    let mapped = if folded.chars().ne([c]) { folded } else { [c].into_iter().nfkd().collect() };
    if mapped.chars().eq([c]) {
        out.push(c);
    } else {
        mapped.chars().for_each(|c| decompose(c, out));
    }
}

/// A default ignorable code point which utf8proc 2.10 (Unicode 16.0)
/// strips.
fn is_ignorable(c: char) -> bool {
    matches!(c,
        '\u{ad}' | '\u{34f}' | '\u{61c}' | '\u{115f}'..='\u{1160}' | '\u{17b4}'..='\u{17b5}'
        | '\u{180b}'..='\u{180f}' | '\u{200b}'..='\u{200f}' | '\u{202a}'..='\u{202e}'
        | '\u{2060}'..='\u{2064}' | '\u{2066}'..='\u{206f}' | '\u{3164}' | '\u{fe00}'..='\u{fe0f}'
        | '\u{feff}' | '\u{ffa0}' | '\u{1bca0}'..='\u{1bca3}' | '\u{1d173}'..='\u{1d17a}'
        | '\u{e0001}' | '\u{e0020}'..='\u{e007f}' | '\u{e0100}'..='\u{e01ef}')
}

/// The character of JIS X 0208 for one which Windows, and the Encoding
/// Standard, map it to instead (see [`crate::jis::jis0208`]).
fn jis_from_windows(c: char) -> char {
    match c {
        '\u{ff5e}' => '\u{301c}', /* WAVE DASH */
        '\u{2225}' => '\u{2016}', /* DOUBLE VERTICAL LINE */
        '\u{2015}' => '\u{2014}', /* EM DASH */
        '\u{ff0d}' => '\u{2212}', /* MINUS SIGN */
        '\u{ffe0}' => '\u{a2}',   /* CENT SIGN */
        '\u{ffe1}' => '\u{a3}',   /* POUND SIGN */
        '\u{ffe2}' => '\u{ac}',   /* NOT SIGN */
        c => c,
    }
}

#[cfg(test)]
mod tests {
    use super::{Fold, field_match};

    fn matches(text: &str, value: &str, case_sensitive: bool) -> bool {
        field_match(text.as_bytes(), value.as_bytes(), if case_sensitive { Fold::None } else { Fold::Case })
    }

    fn char_fold(text: &str, value: &str) -> bool {
        field_match(text.as_bytes(), value.as_bytes(), Fold::Char)
    }

    #[test]
    fn folding_chars() {
        /* width, and compatibility */
        assert!(char_fold("ＡＢＣ", "abc"));
        assert!(char_fold("ｱｲｳ", "アイウ"));
        assert!(char_fold("ﬁle", "FILE"));
        assert!(char_fold("①", "1"));
        assert!(char_fold("Straße", "STRASSE"));
        /* default ignorable code points */
        assert!(char_fold("a\u{200b}b", "ab"));
        assert!(char_fold("葛\u{e0100}飾", "葛飾"));
        /* the code points of Windows for JIS X 0208 */
        for (jis, windows) in [("〜", "～"), ("‖", "∥"), ("—", "―"), ("−", "－"), ("¢", "￠"), ("£", "￡"), ("¬", "￢")] {
            assert!(char_fold(&format!("a{jis}b"), windows), "{jis}");
            assert!(char_fold(&format!("a{windows}b"), jis), "{windows}");
        }
        assert!(!char_fold("〜", "~"));
        /* only ignorable: byte by byte */
        assert!(char_fold("a\u{200b}", "\u{200b}"));
        assert!(!char_fold("ab", "\u{200b}"));
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
        assert!(field_match(b"\xffHello", b"hello", Fold::Case));
        assert!(field_match(b"\xc3\x84rger", b"\xc3\x84R", Fold::Case));
        assert!(!field_match(b"\xff\xc3\xa4", "Ä".as_bytes(), Fold::Case));
        assert!(field_match(b"abc", b"", Fold::Case));
    }
}
