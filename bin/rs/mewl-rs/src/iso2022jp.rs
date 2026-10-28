//! ISO-2022-JP (RFC 1468), ISO-2022-JP-1 (RFC 2237), and ISO-2022-JP-2
//! (RFC 1554) to UTF-8, by two sets of [`Rules`].
//!
//! The graphic character sets are those of ISO-2022-JP-2: ASCII, JIS X
//! 0201 Roman and Katakana, JIS X 0208, JIS X 0212, GB 2312, and KS C
//! 5601 designated to G0, and the upper halves of ISO 8859-1 and ISO
//! 8859-7 designated to G2 and used by the single shift `ESC N`. A
//! double-byte set is decoded by the corresponding multibyte encoding
//! of encoding_rs (EUC-JP, GBK, and EUC-KR), whose code is that of the
//! set with the top bits set.
//!
//! # [`Rules::Glibc`]
//!
//! For `mewl-rs-compat`, the decoder follows what the iconv of glibc
//! does for mewl:
//!
//! - ISO-2022-JP knows ASCII, JIS X 0201 Roman, and JIS X 0208 only;
//!   ISO-2022-JP-2 knows them all. glibc has no ISO-2022-JP-1.
//! - An escape sequence not known is not one: `ESC` stands for itself.
//!   An escape sequence may be incomplete at the end.
//! - A control character, space, and DEL stand for themselves in any
//!   set. A byte with the top bit set is invalid.
//! - The tables of glibc differ from those of encoding_rs in a few
//!   characters, e.g. JIS X 0208 0x2141 is U+301C rather than U+FF5E,
//!   and lack some extensions, e.g. the NEC special characters in row
//!   13 of JIS X 0208. These are patched below.
//! - An invalid byte does not change the state, and a double-byte
//!   character missing its second byte at the end is incomplete.
//!
//! # [`Rules::Rfc`]
//!
//! For `mewl-rs`, every set of ISO-2022-JP-2 is known whatever the
//! charset is named, since an escape sequence tells which set follows.
//! The tables are those of encoding_rs, so JIS X 0208 has the
//! extensions of Windows, e.g. the NEC special characters; but GB 2312
//! is not extended to GBK, and KS C 5601 has U+327E of KS X 1001:2002,
//! as glibc does. What is invalid is replaced with U+FFFD: a byte with
//! the top bit set, a character not in its set, and an escape sequence
//! not known, which is skipped as a whole as ISO 2022 defines it. The
//! characters of the set it designates, unknown, are replaced too: a
//! double-byte set, such as JIS X 0213, is told by `$`.

use encoding_rs::{EUC_JP, EUC_KR, Encoding, GBK, ISO_8859_7};

use crate::mime::Rules;

const ESC: u8 = 0x1b;

/// The charsets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Variant {
    Jp,
    Jp1,
    Jp2,
    /// ISO-2022-JP-3, with JIS X 0213, which is not supported but as
    /// far as it is ISO-2022-JP.
    Jp3,
}

impl Variant {
    /// The variant for a charset name as iconv takes it.
    pub fn for_charset(charset: &[u8]) -> Option<Self> {
        match charset.to_ascii_lowercase().as_slice() {
            b"iso-2022-jp" | b"csiso2022jp" | b"iso2022jp" => Some(Variant::Jp),
            b"iso-2022-jp-1" => Some(Variant::Jp1),
            b"iso-2022-jp-2" | b"csiso2022jp2" | b"iso2022jp2" => Some(Variant::Jp2),
            b"iso-2022-jp-3" => Some(Variant::Jp3),
            _ => None,
        }
    }
}

/// The set designated to G0.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum G0 {
    Ascii,
    Roman,
    Kana,
    Jis0208,
    Jis0212,
    Gb2312,
    Ksc5601,
    /// Not known, double-byte or not (by [`Rules::Rfc`] only).
    Unknown { double: bool },
}

/// The set designated to G2.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum G2 {
    None,
    Latin1,
    Greek,
}

/// What the bytes at a position are.
enum Step {
    /// A character, or nothing for a designation, and the length.
    Char(Option<char>, usize),
    /// The bytes of this length are invalid.
    Invalid(usize),
    /// The input ends in the middle of a sequence.
    Incomplete,
}

struct Decoder {
    rules: Rules,
    /// Whether all the sets of ISO-2022-JP-2 are known.
    jp2: bool,
    g0: G0,
    g2: G2,
}

impl Decoder {
    fn new(variant: Variant, rules: Rules) -> Self {
        let jp2 = rules == Rules::Rfc || variant == Variant::Jp2;
        Decoder { rules, jp2, g0: G0::Ascii, g2: G2::None }
    }

    fn glibc(&self) -> bool {
        self.rules == Rules::Glibc
    }

    /// Decodes the bytes at `pos`, a designation changing the state.
    fn step(&mut self, input: &[u8], pos: usize) -> Step {
        let c = input[pos];
        if c == ESC
            && let Some(step) = self.escape(&input[pos..])
        {
            return step;
        }
        if c >= 0x80 {
            return Step::Invalid(1);
        }
        if self.g0 == G0::Ascii || c < 0x21 || c == 0x7f {
            return Step::Char(Some(char::from(c)), 1);
        }
        match self.g0 {
            G0::Roman => Step::Char(Some(roman(c)), 1),
            G0::Kana if c <= 0x5f => Step::Char(char::from_u32(0xff61 + u32::from(c) - 0x21), 1),
            G0::Kana | G0::Unknown { double: false } => Step::Invalid(1),
            set => self.double(set, &input[pos..]),
        }
    }

    /// An escape sequence at the beginning of `s`. `None` if it is not
    /// one, and `ESC` stands for itself.
    fn escape(&mut self, s: &[u8]) -> Option<Step> {
        let at = |i: usize| s.get(i).copied();
        if self.glibc() && (s.len() < 3 || (self.jp2 && s[1] == b'$' && s[2] == b'(' && s.len() < 4)) {
            /* glibc waits for more. */
            return Some(Step::Incomplete);
        }
        let g0 = match (at(1), at(2), at(3)) {
            (Some(b'('), Some(b'B'), _) => Some((G0::Ascii, 3)),
            (Some(b'('), Some(b'J'), _) => Some((G0::Roman, 3)),
            (Some(b'('), Some(b'I'), _) if self.jp2 => Some((G0::Kana, 3)),
            (Some(b'$'), Some(b'@' | b'B'), _) => Some((G0::Jis0208, 3)),
            (Some(b'$'), Some(b'A'), _) if self.jp2 => Some((G0::Gb2312, 3)),
            (Some(b'$'), Some(b'('), Some(b'C')) if self.jp2 => Some((G0::Ksc5601, 4)),
            (Some(b'$'), Some(b'('), Some(b'D')) if self.jp2 => Some((G0::Jis0212, 4)),
            _ => None,
        };
        if let Some((g0, len)) = g0 {
            self.g0 = g0;
            return Some(Step::Char(None, len));
        }
        if self.jp2 {
            match (at(1), at(2)) {
                (Some(b'.'), Some(b'A')) => {
                    self.g2 = G2::Latin1;
                    return Some(Step::Char(None, 3));
                }
                (Some(b'.'), Some(b'F')) => {
                    self.g2 = G2::Greek;
                    return Some(Step::Char(None, 3));
                }
                (Some(b'N'), Some(c)) => return Some(self.single_shift(c)),
                _ => {}
            }
        }
        if self.glibc() {
            return None;
        }
        /* ESC, intermediate bytes, and a final byte (ISO 2022). */
        let mid = s[1..].iter().take_while(|&&c| (0x20..=0x2f).contains(&c)).count();
        let len = match at(1 + mid) {
            Some(0x30..=0x7e) => 2 + mid,
            _ => 1 + mid,
        };
        if len > 1 + mid && mid > 0 && b"($".contains(&s[1]) {
            /* a designation to G0 of a set not known */
            self.g0 = G0::Unknown { double: s[1] == b'$' };
        }
        Some(Step::Invalid(len))
    }

    /// `ESC N` and `c`, a character of G2.
    fn single_shift(&self, c: u8) -> Step {
        let ch = match self.g2 {
            G2::Latin1 if self.glibc() => char::from(c | 0x80),
            G2::Latin1 if (0x20..0x80).contains(&c) => char::from(c | 0x80),
            G2::Greek if (0x20..0x80).contains(&c) => match one(ISO_8859_7, &[c | 0x80]) {
                Some(ch) => ch,
                None => return Step::Invalid(if self.glibc() { 1 } else { 3 }),
            },
            _ => return Step::Invalid(if self.glibc() { 1 } else { 3 }),
        };
        Step::Char(Some(ch), 3)
    }

    /// A character of the double-byte `set` at the beginning of `s`.
    fn double(&self, set: G0, s: &[u8]) -> Step {
        let b1 = s[0];
        /* glibc checks the first byte, but of KS C 5601, first. */
        let lead_ok = match set {
            G0::Jis0212 => (0x22..=0x6d).contains(&b1),
            G0::Gb2312 => b1 <= 0x77,
            _ => true,
        };
        if self.glibc() && !lead_ok {
            return Step::Invalid(1);
        }
        let Some(&b2) = s.get(1) else {
            return if self.glibc() { Step::Incomplete } else { Step::Invalid(1) };
        };
        if !(0x21..=0x7e).contains(&b2) {
            return Step::Invalid(1);
        }
        let ch = match set {
            _ if !lead_ok => None,
            G0::Jis0208 => self.jis0208(b1, b2),
            G0::Jis0212 => one(EUC_JP, &[0x8f, b1 | 0x80, b2 | 0x80]),
            G0::Gb2312 => self.gb2312(b1, b2),
            G0::Ksc5601 => self.ksc5601(b1, b2),
            _ => None,
        };
        match ch {
            Some(ch) => Step::Char(Some(ch), 2),
            None if self.glibc() => Step::Invalid(1),
            None => Step::Invalid(2),
        }
    }

    fn jis0208(&self, b1: u8, b2: u8) -> Option<char> {
        if self.glibc() {
            /* the NEC special characters and the IBM extensions */
            if b1 == 0x2d || b1 >= 0x75 {
                return None;
            }
            let ch = match (b1, b2) {
                (0x21, 0x41) => '\u{301c}',
                (0x21, 0x42) => '\u{2016}',
                (0x21, 0x5d) => '\u{2212}',
                (0x21, 0x71) => '\u{a2}',
                (0x21, 0x72) => '\u{a3}',
                (0x22, 0x4c) => '\u{ac}',
                _ => return one(EUC_JP, &[b1 | 0x80, b2 | 0x80]),
            };
            return Some(ch);
        }
        one(EUC_JP, &[b1 | 0x80, b2 | 0x80])
    }

    fn gb2312(&self, b1: u8, b2: u8) -> Option<char> {
        /* GBK has more, e.g. user-defined areas. */
        const NOT_IN_GB2312: &[(u8, u8, u8)] = &[
            (0x22, 0x21, 0x30),
            (0x22, 0x63, 0x64),
            (0x22, 0x6f, 0x70),
            (0x22, 0x7d, 0x7e),
            (0x24, 0x74, 0x7e),
            (0x25, 0x77, 0x7e),
            (0x26, 0x39, 0x40),
            (0x26, 0x59, 0x7e),
            (0x27, 0x42, 0x50),
            (0x27, 0x72, 0x7e),
            (0x28, 0x3b, 0x44),
            (0x28, 0x6a, 0x7e),
            (0x29, 0x21, 0x23),
            (0x29, 0x70, 0x7e),
            (0x57, 0x7a, 0x7e),
        ];
        if (0x2a..=0x2f).contains(&b1)
            || b1 >= 0x78
            || NOT_IN_GB2312.iter().any(|&(r, lo, hi)| b1 == r && (lo..=hi).contains(&b2))
        {
            return None;
        }
        match (b1, b2) {
            (0x21, 0x24) if self.glibc() => Some('\u{30fb}'),
            (0x21, 0x2a) if self.glibc() => Some('\u{2015}'),
            _ => one(GBK, &[b1 | 0x80, b2 | 0x80]),
        }
    }

    fn ksc5601(&self, b1: u8, b2: u8) -> Option<char> {
        if (b1, b2) == (0x22, 0x68) {
            /* added to KS X 1001 in 2002 */
            return Some('\u{327e}');
        }
        one(EUC_KR, &[b1 | 0x80, b2 | 0x80])
    }
}

/// A character of JIS X 0201 Roman.
fn roman(c: u8) -> char {
    match c {
        b'\\' => '\u{a5}',
        b'~' => '\u{203e}',
        c => char::from(c),
    }
}

/// `bytes` decoded into one character, `None` if invalid.
fn one(encoding: &'static Encoding, bytes: &[u8]) -> Option<char> {
    let text = encoding.decode_without_bom_handling_and_without_replacement(bytes)?;
    let mut chars = text.chars();
    let ch = chars.next()?;
    chars.next().is_none().then_some(ch)
}

/// Converts `input` to UTF-8 by `rules`. With `keep_invalid`, what is
/// invalid is kept as it is by [`Rules::Glibc`], and so is the rest of
/// an incomplete sequence at the end, or replaced with U+FFFD by
/// [`Rules::Rfc`], and the conversion goes on; otherwise `None` is
/// returned for it.
pub fn convert(variant: Variant, rules: Rules, input: &[u8], keep_invalid: bool) -> Option<Vec<u8>> {
    let mut decoder = Decoder::new(variant, rules);
    let mut out = String::with_capacity(input.len() * 2);
    let mut kept = Vec::new();
    let mut pos = 0;
    while pos < input.len() {
        match decoder.step(input, pos) {
            Step::Char(ch, len) => {
                out.extend(ch);
                pos += len;
            }
            _ if !keep_invalid => return None,
            Step::Invalid(len) if rules == Rules::Rfc => {
                out.push(char::REPLACEMENT_CHARACTER);
                pos += len;
            }
            /* mewl keeps a byte and goes on. */
            Step::Invalid(_) => {
                kept.extend_from_slice(out.as_bytes());
                kept.push(input[pos]);
                out.clear();
                pos += 1;
            }
            Step::Incomplete => {
                kept.extend_from_slice(out.as_bytes());
                kept.extend_from_slice(&input[pos..]);
                return Some(kept);
            }
        }
    }
    kept.extend_from_slice(out.as_bytes());
    Some(kept)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn glibc(variant: Variant, s: &[u8]) -> Option<Vec<u8>> {
        convert(variant, Rules::Glibc, s, false).or_else(|| convert(variant, Rules::Glibc, s, true))
    }

    fn rfc(s: &[u8]) -> String {
        String::from_utf8(convert(Variant::Jp, Rules::Rfc, s, true).unwrap()).unwrap()
    }

    #[test]
    fn sets() {
        let jp2 = |s: &[u8]| String::from_utf8(glibc(Variant::Jp2, s).unwrap()).unwrap();
        assert_eq!(jp2(b"\x1b$BF|K\\8l\x1b(B"), "日本語");
        assert_eq!(jp2(b"\x1b$@F|\x1b(J\\~\x1b(B\\"), "日¥‾\\");
        assert_eq!(jp2(b"Goj\x1b$(D+W\x1b(B"), "Gojō");
        assert_eq!(jp2(b"\x1b$AVPND\x1b(B"), "中文");
        assert_eq!(jp2(b"\x1b$(CGQ19\x1b(B"), "한국");
        assert_eq!(jp2(b"\x1b(I12\x1b(B"), "ｱｲ");
        assert_eq!(jp2(b"\x1b.Acaf\x1bNi\x1b.F\x1bNa"), "café\u{3b1}");
        /* Space and controls in a double-byte set. */
        assert_eq!(jp2(b"\x1b$BF| K\\\x1b(B"), "日 本");
    }

    #[test]
    fn glibc_tables() {
        let jp = |s: &[u8]| glibc(Variant::Jp, s).unwrap();
        assert_eq!(jp(b"\x1b$B!A\x1b(B"), "\u{301c}".as_bytes());
        /* No NEC special character nor JIS X 0212 in ISO-2022-JP. */
        assert_eq!(jp(b"\x1b$B-!\x1b(B"), b"-!");
        assert_eq!(jp(b"Goj\x1b$(D+W\x1b(B"), b"Goj\x1b$(D+W");
        assert_eq!(glibc(Variant::Jp2, b"\x1b$A!$\x1b(B").unwrap(), "\u{30fb}".as_bytes());
        assert_eq!(glibc(Variant::Jp2, b"\x1b$(C\"h\x1b(B").unwrap(), "\u{327e}".as_bytes());
    }

    #[test]
    fn glibc_invalid() {
        let jp = |s: &[u8]| glibc(Variant::Jp, s).unwrap();
        /* An invalid byte keeps the state. */
        assert_eq!(jp(b"\x1b$B\xffF|\x1b(B"), [&b"\xff"[..], "日".as_bytes()].concat());
        /* Incomplete at the end. */
        assert_eq!(jp(b"\x1b$BF|F"), "日F".as_bytes());
        assert_eq!(jp(b"a\x1b$"), b"a\x1b$");
        assert_eq!(convert(Variant::Jp, Rules::Glibc, b"\x1b$B-!", false), None);
    }

    #[test]
    fn rfc_any_set_and_replacement() {
        /* Whatever the charset, with the extensions of Windows. */
        assert_eq!(rfc(b"Goj\x1b$(D+W\x1b(B"), "Gojō");
        assert_eq!(rfc(b"\x1b$B-!\x1b(B"), "①");
        assert_eq!(rfc(b"\x1b$B!A\x1b(B"), "\u{ff5e}");
        /* JIS X 0213, not known, as a whole. */
        assert_eq!(rfc(b"a\x1b$(Q.!.\"\x1b(Bb"), "a\u{fffd}\u{fffd}\u{fffd}b");
        assert_eq!(rfc(b"\x1b$BF|\xff\x1b(B"), "日\u{fffd}");
        assert_eq!(rfc(b"\x1b$BF"), "\u{fffd}");
        assert_eq!(rfc(b"\x1bZa"), "\u{fffd}a");
        /* GB 2312, not GBK, and KS X 1001:2002 */
        assert_eq!(rfc(b"\x1b$A!$*!\x1b(B"), "\u{b7}\u{fffd}");
        assert_eq!(rfc(b"\x1b$(C\"h\x1b(B"), "\u{327e}");
    }
}
