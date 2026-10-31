//! Multibyte charsets of the Encoding Standard with a few characters as
//! Mew, that is Emacs, decodes them, for `mewl-rs`.
//!
//! encoding_rs decodes GB 2312 as GBK, Big5 as Big5-HKSCS, and EUC-KR
//! as the code page 949 of Windows, which agree with Emacs but in a few
//! characters, while Emacs maps many characters of their extensions to
//! the Private Use Area, which nobody can type: so only those that
//! Emacs maps to other characters of Unicode are taken as Emacs does.

use encoding_rs::{BIG5, EUC_KR, Encoding, GB18030, GBK};

/// The charsets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Patch {
    /// GB 2312 (EUC-CN), by GBK: what Emacs guesses, and names such as
    /// `euc-cn`, which Mew does not know. Mew decodes `gb2312` and
    /// `cn-gb` as GBK itself.
    Gb2312,
    Gb18030,
    /// EUC-KR, by the code page 949.
    EucKr,
    /// Big5, by Big5-HKSCS.
    Big5,
}

impl Patch {
    /// The encoding of encoding_rs for the charset.
    pub fn encoding(self) -> &'static Encoding {
        match self {
            Patch::Gb2312 => GBK,
            Patch::Gb18030 => GB18030,
            Patch::EucKr => EUC_KR,
            Patch::Big5 => BIG5,
        }
    }

    /// The character which Emacs takes `code` for, where it differs.
    fn char_for(self, code: &[u8]) -> Option<char> {
        let ch = match (self, code) {
            (Patch::Gb2312, [0xa1, 0xa4]) => '\u{30fb}', /* KATAKANA MIDDLE DOT, not U+00B7 */
            (Patch::Gb2312, [0xa1, 0xaa]) => '\u{2015}', /* HORIZONTAL BAR, not U+2014 */
            /* characters of CJK Extension B, not of the Private Use Area */
            (Patch::Gb18030, [0xfe, 0x51]) => '\u{20087}',
            (Patch::Gb18030, [0xfe, 0x52]) => '\u{20089}',
            (Patch::Gb18030, [0xfe, 0x53]) => '\u{200cc}',
            (Patch::Gb18030, [0xfe, 0x6c]) => '\u{215d7}',
            (Patch::Gb18030, [0xfe, 0x76]) => '\u{2298f}',
            (Patch::Gb18030, [0xfe, 0x91]) => '\u{241fe}',
            (Patch::EucKr, [0xa2, 0xe8]) => '\u{327e}', /* added to KS X 1001 in 2002 */
            (Patch::Big5, [0xf9, 0xfe]) => '\u{2593}',  /* DARK SHADE, not U+FFED */
            _ => return None,
        };
        Some(ch)
    }

    /// The length of the character at the beginning of `s`, as far as the
    /// Encoding Standard takes it, at least 1.
    fn char_len(self, s: &[u8]) -> usize {
        let at = |i: usize, range: std::ops::RangeInclusive<u8>| s.get(i).is_some_and(|c| range.contains(c));
        if !at(0, 0x81..=0xfe) {
            return 1;
        }
        match self {
            Patch::Gb2312 | Patch::Gb18030 if at(1, 0x30..=0x39) && at(2, 0x81..=0xfe) && at(3, 0x30..=0x39) => 4,
            Patch::Gb2312 | Patch::Gb18030 if at(1, 0x40..=0x7e) || at(1, 0x80..=0xfe) => 2,
            Patch::EucKr if at(1, 0x41..=0xfe) => 2,
            Patch::Big5 if at(1, 0x40..=0x7e) || at(1, 0xa1..=0xfe) => 2,
            _ => 1,
        }
    }
}

/// Converts `input` to UTF-8 by encoding_rs, but the characters of
/// [`Patch`]. With `keep_invalid`, what is invalid is replaced with
/// U+FFFD; otherwise it fails.
pub fn convert(patch: Patch, input: &[u8], keep_invalid: bool) -> Option<Vec<u8>> {
    decode_with(patch.encoding(), input, keep_invalid, |s| {
        let len = patch.char_len(s);
        (len, patch.char_for(&s[..len]))
    })
}

/// Converts `input` to UTF-8 by `encoding`, but the characters that
/// `step` gives: walking the characters as the Encoding Standard does,
/// it tells the length of the one at the beginning of a slice and the
/// character to take it for, if it is not that of `encoding`. With
/// `keep_invalid`, what is invalid is replaced with U+FFFD; otherwise
/// it fails.
///
/// The characters before such a character are decoded one by one, so
/// that what is incomplete before it is invalid by itself, as in the
/// whole input, rather than at the end of a piece; the rest is decoded
/// as a whole.
pub fn decode_with(
    encoding: &'static Encoding,
    input: &[u8],
    keep_invalid: bool,
    step: impl Fn(&[u8]) -> (usize, Option<char>),
) -> Option<Vec<u8>> {
    let mut out = String::with_capacity(input.len() * 2);
    let push = |out: &mut String, bytes: &[u8]| {
        let (text, invalid) = encoding.decode_without_bom_handling(bytes);
        out.push_str(&text);
        !invalid || keep_invalid
    };
    let mut chars = Vec::new();
    let mut beg = 0;
    let mut pos = 0;
    while pos < input.len() {
        let (len, ch) = step(&input[pos..]);
        let end = (pos + len).min(input.len());
        if let Some(ch) = ch {
            for (b, e) in chars.drain(..) {
                if !push(&mut out, &input[b..e]) {
                    return None;
                }
            }
            out.push(ch);
            beg = end;
        } else {
            chars.push((pos, end));
        }
        pos = end;
    }
    push(&mut out, &input[beg..]).then(|| out.into_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conv(patch: Patch, input: &[u8]) -> String {
        String::from_utf8(convert(patch, input, true).unwrap()).unwrap()
    }

    #[test]
    fn as_emacs() {
        assert_eq!(conv(Patch::Gb2312, b"\xa1\xa4\xa1\xaa\xd6\xd0"), "・―中");
        assert_eq!(conv(Patch::Gb18030, b"\xfe\x51\x81\x30\x81\x30\xa1\xa4"), "\u{20087}\u{80}·");
        assert_eq!(conv(Patch::EucKr, b"\xa2\xe8\xc7\xd1"), "㉾한");
        assert_eq!(conv(Patch::Big5, b"\xf9\xfe\xa4\xa4"), "▓中");
        /* What is invalid is as the Encoding Standard takes it. */
        assert_eq!(conv(Patch::Gb2312, b"\xa1\x30\xa1\xa4\xff"), "\u{fffd}0・\u{fffd}");
        assert_eq!(conv(Patch::Big5, b"\xa1\x30\xf9\xfe"), "\u{fffd}0▓");
        /* incomplete before a character patched, and at the end */
        assert_eq!(conv(Patch::Gb18030, b"\x81\x30\xfe\x51\x81\x30"), "\u{fffd}0\u{20087}\u{fffd}");
        assert_eq!(convert(Patch::Big5, b"\xf9\xfe\xff", false), None);
    }
}
