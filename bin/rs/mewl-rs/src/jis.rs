//! Shift_JIS and EUC-JP, and CP932 and eucJP-ms of Microsoft, to UTF-8
//! by two sets of [`Rules`].
//!
//! # [`Rules::Glibc`]
//!
//! For `mewl-rs-compat`, as the iconv of glibc converts them for mewl.
//! A character is decoded by encoding_rs (the Encoding Standard),
//! whose Shift_JIS is CP932 and whose EUC-JP has the extensions of
//! Windows too, and patched where glibc differs:
//!
//! - In Shift_JIS and EUC-JP, six characters of JIS X 0208 are those of
//!   JIS, not of Windows ([`jis0208`]), e.g. 0x2141 is U+301C (WAVE
//!   DASH) rather than U+FF5E (FULLWIDTH TILDE); the NEC special
//!   characters (row 13), the IBM extensions, and the user-defined area
//!   are invalid. In Shift_JIS, 0x5C and 0x7E are those of JIS X 0201
//!   Roman, U+00A5 and U+203E.
//! - In CP932 and Shift_JIS, 0x80 is invalid.
//! - In EUC-JP and eucJP-ms, 0x80 to 0x9F but SS2 and SS3 are C1
//!   controls.
//! - In eucJP-ms, the user-defined area, rows 85 to 94 of JIS X 0208
//!   and of JIS X 0212, is mapped to the Private Use Area, and the IBM
//!   extensions not in JIS X 0212 are in rows 83 and 84 of JIS X 0212
//!   ([`glibc::EUC_JP_MS_IBM`]).
//!
//! As mewl does, a byte which is not a character is invalid, and the
//! conversion goes on from the next byte.
//!
//! # [`Rules::Rfc`]
//!
//! For `mewl-rs`, as the Encoding Standard converts them, but seven
//! characters of JIS X 0208 in Shift_JIS and EUC-JP, which are those of
//! JIS as Emacs takes them ([`jis0208`]), so that what Mew shows is
//! found; CP932 and eucJP-ms keep those of Windows, as in Emacs. What
//! is invalid is replaced with U+FFFD.

use encoding_rs::{EUC_JP, Encoding, SHIFT_JIS};

use crate::charset::glibc;
use crate::mime::Rules;

/// The charsets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Variant {
    ShiftJis,
    Cp932,
    EucJp,
    EucJpMs,
}

impl Variant {
    /// The encoding of encoding_rs for the charset.
    pub fn encoding(self) -> &'static Encoding {
        match self {
            Variant::ShiftJis | Variant::Cp932 => SHIFT_JIS,
            Variant::EucJp | Variant::EucJpMs => EUC_JP,
        }
    }
}

/// What the bytes at a position are.
enum Step {
    /// A character and its length.
    Char(char, usize),
    /// The byte is invalid.
    Invalid,
    /// The input ends in the middle of a character.
    Incomplete,
}

/// Converts `input` to UTF-8 by `rules`. With `keep_invalid`, an
/// invalid byte is kept as it is by [`Rules::Glibc`], and so is the
/// rest of an incomplete character at the end, or replaced with U+FFFD
/// by [`Rules::Rfc`]; otherwise it fails.
pub fn convert(variant: Variant, rules: Rules, input: &[u8], keep_invalid: bool) -> Option<Vec<u8>> {
    if rules == Rules::Rfc {
        return convert_rfc(variant, input, keep_invalid);
    }
    let mut out = Vec::with_capacity(input.len() * 2);
    let mut pos = 0;
    while pos < input.len() {
        let s = &input[pos..];
        let step = match variant {
            Variant::ShiftJis | Variant::Cp932 => shift_jis(variant, s),
            Variant::EucJp | Variant::EucJpMs => euc_jp(variant, s),
        };
        match step {
            Step::Char(c, len) => {
                out.extend_from_slice(c.encode_utf8(&mut [0; 4]).as_bytes());
                pos += len;
            }
            _ if !keep_invalid => return None,
            Step::Invalid => {
                out.push(input[pos]);
                pos += 1;
            }
            Step::Incomplete => {
                out.extend_from_slice(s);
                break;
            }
        }
    }
    Some(out)
}

/// [`convert`] by [`Rules::Rfc`]: by encoding_rs, but the seven
/// characters of [`jis0208`] in Shift_JIS and EUC-JP, found by walking
/// the characters as the Encoding Standard does.
fn convert_rfc(variant: Variant, input: &[u8], keep_invalid: bool) -> Option<Vec<u8>> {
    let encoding = variant.encoding();
    let patched = matches!(variant, Variant::ShiftJis | Variant::EucJp);
    let mut out = String::with_capacity(input.len() * 2);
    let mut beg = 0;
    let mut pos = 0;
    while pos < input.len() {
        let len = char_len(variant, &input[pos..]);
        let jis = match (variant, input.get(pos..pos + 2)) {
            _ if !patched || len != 2 => None,
            (Variant::ShiftJis, Some(&[lead, trail])) => sjis_to_jis(lead, trail),
            (_, Some(&[row, col])) if row >= 0xa1 => Some((row & 0x7f, col & 0x7f)),
            _ => None,
        };
        if let Some(ch) = jis.and_then(|(b1, b2)| jis0208(b1, b2, Rules::Rfc)) {
            let (text, invalid) = encoding.decode_without_bom_handling(&input[beg..pos]);
            if invalid && !keep_invalid {
                return None;
            }
            out.push_str(&text);
            out.push(ch);
            beg = pos + len;
        }
        pos += len;
    }
    let (text, invalid) = encoding.decode_without_bom_handling(&input[beg..]);
    if invalid && !keep_invalid {
        return None;
    }
    out.push_str(&text);
    Some(out.into_bytes())
}

/// The length of the character at the beginning of `s`, as far as the
/// Encoding Standard takes it, at least 1.
fn char_len(variant: Variant, s: &[u8]) -> usize {
    let trail = |i: usize, range: std::ops::RangeInclusive<u8>| s.get(i).is_some_and(|c| range.contains(c));
    match variant {
        Variant::ShiftJis | Variant::Cp932 => match s[0] {
            0x81..=0x9f | 0xe0..=0xfc if trail(1, 0x40..=0x7e) || trail(1, 0x80..=0xfc) => 2,
            _ => 1,
        },
        Variant::EucJp | Variant::EucJpMs => match s[0] {
            0x8f if trail(1, 0xa1..=0xfe) && trail(2, 0xa1..=0xfe) => 3,
            0x8e | 0xa1..=0xfe if trail(1, 0xa1..=0xfe) => 2,
            _ => 1,
        },
    }
}

fn shift_jis(variant: Variant, s: &[u8]) -> Step {
    let glibc_sjis = variant == Variant::ShiftJis;
    match s[0] {
        b'\\' if glibc_sjis => Step::Char('\u{a5}', 1),
        b'~' if glibc_sjis => Step::Char('\u{203e}', 1),
        c @ 0x00..=0x7f => Step::Char(char::from(c), 1),
        c @ 0xa1..=0xdf => Step::Char(char::from_u32(0xff61 + u32::from(c) - 0xa1).unwrap_or_default(), 1),
        lead @ (0x81..=0x9f | 0xe0..=0xfc) => {
            let Some(&trail) = s.get(1) else {
                return Step::Incomplete;
            };
            /* the NEC special characters, the IBM extensions, and the user-defined area */
            if glibc_sjis && (lead == 0x87 || lead == 0xed || lead == 0xee || lead >= 0xf0) {
                return Step::Invalid;
            }
            let ch = match sjis_to_jis(lead, trail) {
                Some((b1, b2)) if glibc_sjis => jis0208(b1, b2, Rules::Glibc),
                _ => None,
            };
            match ch.or_else(|| one(SHIFT_JIS, &[lead, trail])) {
                Some(ch) => Step::Char(ch, 2),
                None => Step::Invalid,
            }
        }
        _ => Step::Invalid,
    }
}

fn euc_jp(variant: Variant, s: &[u8]) -> Step {
    let ms = variant == Variant::EucJpMs;
    let len = match s[0] {
        c @ 0x00..=0x7f => return Step::Char(char::from(c), 1),
        0x8e => 2,
        0x8f => 3,
        c @ 0x80..=0x9f => return Step::Char(char::from(c), 1),
        0xa1..=0xfe => 2,
        _ => return Step::Invalid,
    };
    /* glibc checks the second byte before it waits for the third. */
    match s.get(1) {
        None => return Step::Incomplete,
        Some(0..0xa1) => return Step::Invalid,
        _ => {}
    }
    let Some(s) = s.get(..len) else {
        return Step::Incomplete;
    };
    if s[0] == 0x8e {
        /* JIS X 0201 Katakana */
        return match one(EUC_JP, s) {
            Some(ch) => Step::Char(ch, len),
            None => Step::Invalid,
        };
    }
    let (row, col) = (s[len - 2], s[len - 1]);
    if !(0xa1..=0xfe).contains(&row) || !(0xa1..=0xfe).contains(&col) {
        return Step::Invalid;
    }
    let user_defined = |base: u32| char::from_u32(base + (u32::from(row) - 0xf5) * 94 + u32::from(col) - 0xa1);
    let ch = match (len, row) {
        /* JIS X 0212 */
        (3, 0xf5..) if ms => user_defined(0xe3ac),
        (3, 0xf3 | 0xf4) if ms => glibc::EUC_JP_MS_IBM
            .binary_search_by_key(&(u16::from(row) << 8 | u16::from(col)), |&(code, _)| code)
            .ok()
            .and_then(|i| char::from_u32(glibc::EUC_JP_MS_IBM[i].1.into())),
        (3, _) if ms && (row, col) == (0xa2, 0xc3) => Some('\u{ffe4}'),
        (3, _) => one(EUC_JP, s),
        /* JIS X 0208 */
        (_, 0xf5..) if ms => user_defined(0xe000),
        (_, 0xad | 0xf9..) if !ms => None,
        _ if !ms => jis0208(row & 0x7f, col & 0x7f, Rules::Glibc).or_else(|| one(EUC_JP, s)),
        _ => one(EUC_JP, s),
    };
    match ch {
        Some(ch) => Step::Char(ch, len),
        None => Step::Invalid,
    }
}

/// The code of JIS X 0208 of a double-byte character of Shift_JIS.
fn sjis_to_jis(lead: u8, trail: u8) -> Option<(u8, u8)> {
    let row = match lead {
        0x81..=0x9f => lead - 0x81,
        0xe0..=0xef => lead - 0xc1,
        _ => return None,
    };
    let (b1, b2) = match trail {
        0x40..=0x7e => (row * 2 + 0x21, trail - 0x1f),
        0x80..=0x9e => (row * 2 + 0x21, trail - 0x20),
        0x9f..=0xfc => (row * 2 + 0x22, trail - 0x7e),
        _ => return None,
    };
    Some((b1, b2))
}

/// The six characters of JIS X 0208 that glibc and Emacs take as JIS
/// defines them while the Encoding Standard takes them as Windows does,
/// e.g. U+FF5E (FULLWIDTH TILDE) for 0x2141 (WAVE DASH), and for
/// [`Rules::Rfc`] 0x213D, U+2014 (EM DASH) in Emacs but U+2015
/// (HORIZONTAL BAR) in glibc and the Encoding Standard.
pub fn jis0208(b1: u8, b2: u8, rules: Rules) -> Option<char> {
    let ch = match (b1, b2) {
        (0x21, 0x3d) if rules == Rules::Rfc => '\u{2014}',
        (0x21, 0x41) => '\u{301c}',
        (0x21, 0x42) => '\u{2016}',
        (0x21, 0x5d) => '\u{2212}',
        (0x21, 0x71) => '\u{a2}',
        (0x21, 0x72) => '\u{a3}',
        (0x22, 0x4c) => '\u{ac}',
        _ => return None,
    };
    Some(ch)
}

/// `bytes` decoded into one character, `None` if invalid.
pub fn one(encoding: &'static Encoding, bytes: &[u8]) -> Option<char> {
    let text = encoding.decode_without_bom_handling_and_without_replacement(bytes)?;
    let mut chars = text.chars();
    let ch = chars.next()?;
    chars.next().is_none().then_some(ch)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conv(variant: Variant, input: &[u8]) -> String {
        String::from_utf8(convert(variant, Rules::Glibc, input, false).unwrap()).unwrap()
    }

    #[test]
    fn shift_jis() {
        assert_eq!(conv(Variant::ShiftJis, b"\x93\xfa\x96\x7b\x81\x60\\~\xb1"), "日本〜¥‾ｱ");
        assert_eq!(conv(Variant::Cp932, b"\x93\xfa\x96\x7b\x81\x60\\~\xb1\x87\x40"), "日本～\\~ｱ①");
        for s in [&b"\x87\x40"[..], b"\xfa\x40", b"\xf0\x40", b"\x80", b"\x81\x7f"] {
            assert_eq!(convert(Variant::ShiftJis, Rules::Glibc, s, false), None);
            assert_eq!(convert(Variant::ShiftJis, Rules::Glibc, s, true).unwrap(), s);
        }
        assert_eq!(convert(Variant::Cp932, Rules::Glibc, b"\x80", false), None);
        /* An invalid byte is kept, and the next one is another character. */
        assert_eq!(convert(Variant::ShiftJis, Rules::Glibc, b"\x81\x7f\x93\xfa", true).unwrap(), b"\x81\x7f\xe6\x97\xa5");
        assert_eq!(convert(Variant::ShiftJis, Rules::Glibc, b"a\x93", true).unwrap(), b"a\x93");
    }

    #[test]
    fn euc_jp() {
        assert_eq!(conv(Variant::EucJp, b"\xc6\xfc\xcb\xdc\xa1\xc1\x8e\xb1\x8f\xa2\xc3\x85"), "日本〜ｱ¦\u{85}");
        assert_eq!(conv(Variant::EucJpMs, b"\xa1\xc1\xad\xa1\x8f\xa2\xc3\x85"), "～①￤\u{85}");
        assert_eq!(conv(Variant::EucJpMs, b"\xf5\xa1\xfe\xfe\x8f\xf5\xa1\x8f\xf3\xf3"), "\u{e000}\u{e3ab}\u{e3ac}ⅰ");
        for s in [&b"\xad\xa1"[..], b"\xf9\xa1", b"\xf5\xa1", b"\xa1\x41", b"\x8e\xe0", b"\xff"] {
            assert_eq!(convert(Variant::EucJp, Rules::Glibc, s, false), None);
            assert_eq!(convert(Variant::EucJp, Rules::Glibc, s, true).unwrap(), s);
        }
        assert_eq!(convert(Variant::EucJp, Rules::Glibc, b"a\x8f\xa2", true).unwrap(), b"a\x8f\xa2");
        assert_eq!(convert(Variant::EucJp, Rules::Glibc, b"\x8f\x80", true).unwrap(), b"\x8f\xc2\x80");
    }

    fn rfc(variant: Variant, input: &[u8]) -> String {
        String::from_utf8(convert(variant, Rules::Rfc, input, true).unwrap()).unwrap()
    }

    #[test]
    fn as_emacs() {
        let sjis = b"\x81\x60\x81\x61\x81\x7c\x81\x91\x81\x92\x81\xca\x81\x5c\\~\x87\x40\xfa\x54\xee\xf9\x80";
        assert_eq!(rfc(Variant::ShiftJis, sjis), "〜‖−¢£¬\u{2014}\\~①￢￢\u{80}");
        assert_eq!(rfc(Variant::Cp932, sjis), "～∥－￠￡￢\u{2015}\\~①￢￢\u{80}");
        let euc = b"\xa1\xc1\xa1\xc2\xa1\xdd\xa1\xf1\xa1\xf2\xa2\xcc\xa1\xbd\xad\xa1\x8f\xa2\xc3";
        assert_eq!(rfc(Variant::EucJp, euc), "〜‖−¢£¬\u{2014}①¦");
        assert_eq!(rfc(Variant::EucJpMs, euc), "～∥－￠￡￢\u{2015}①¦");
        /* glibc takes 0x213D as the Encoding Standard does */
        assert_eq!(conv(Variant::ShiftJis, b"\x81\x5c"), "\u{2015}");
        /* What is invalid is as the Encoding Standard takes it. */
        assert_eq!(rfc(Variant::ShiftJis, b"\x81\x81\x60\x85\x40\x81\x60\x81"), "＝`\u{fffd}@〜\u{fffd}");
        assert_eq!(rfc(Variant::EucJp, b"\xa1\xa1\xa1\x41\xa1\xc1\xff"), "\u{3000}\u{fffd}A〜\u{fffd}");
        assert_eq!(convert(Variant::ShiftJis, Rules::Rfc, b"\x81\x60\x85\x40", false), None);
    }

    #[test]
    fn to_jis() {
        assert_eq!(sjis_to_jis(0x81, 0x60), Some((0x21, 0x41)));
        assert_eq!(sjis_to_jis(0x81, 0xca), Some((0x22, 0x4c)));
        assert_eq!(sjis_to_jis(0xea, 0xa4), Some((0x74, 0x26)));
    }
}
