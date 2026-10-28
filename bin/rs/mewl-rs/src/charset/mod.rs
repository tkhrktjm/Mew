//! Charsets of encoded-words and their conversion to UTF-8.
//!
//! mewl converts a charset with iconv: it renames a few MIME charset
//! names for iconv and, for a few others, retries with a superset when
//! the conversion fails (see [`RENAMED`]). `mewl-rs-compat` does the
//! same, knowing a name as glibc does: [`glibc::NAMES`] lists the names
//! glibc knows for each converter emulated. `mewl-rs` knows the names
//! of the Encoding Standard and, for the converters emulated, those of
//! glibc as well.

mod glibc;

use encoding_rs::{DecoderResult, Encoding};

use crate::iso2022jp;
use crate::mime::Rules;
use crate::utf7;

/// A converter of a charset to UTF-8.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Converter {
    /// By encoding_rs.
    Encoding(&'static Encoding),
    /// ISO-2022-JP and its variants.
    Iso2022Jp(iso2022jp::Variant),
    /// UTF-7 or UTF-7-IMAP, unknown to the Encoding Standard.
    Utf7(utf7::Variant),
}

impl Converter {
    /// Converts `input` to UTF-8 by `rules`. With `keep_invalid`, an
    /// invalid byte sequence is kept as it is by [`Rules::Glibc`], as
    /// mewl does, and replaced with U+FFFD by [`Rules::Rfc`] but an
    /// invalid sequence of UTF-7 in US-ASCII; otherwise it fails.
    fn convert(self, input: &[u8], rules: Rules, keep_invalid: bool) -> Option<Vec<u8>> {
        match self {
            Converter::Encoding(encoding) if keep_invalid && rules == Rules::Rfc => {
                let (text, _) = encoding.decode_without_bom_handling(input);
                Some(text.into_owned().into_bytes())
            }
            Converter::Encoding(encoding) => convert(encoding, input, keep_invalid),
            Converter::Iso2022Jp(variant) => iso2022jp::convert(variant, rules, input, keep_invalid),
            Converter::Utf7(variant) => utf7::convert(variant, rules, input, keep_invalid),
        }
    }
}

/// A MIME charset name that mewl renames for iconv, or retries with a
/// superset of when the conversion fails.
struct Renamed {
    mime: &'static [u8],
    iconv: Option<&'static [u8]>,
    retry: Option<&'static [u8]>,
}

/// The table of mewl, `rfc2047_charsets`.
const RENAMED: [Renamed; 10] = [
    Renamed { mime: b"ks_c_5601-1987", iconv: Some(b"CP949"), retry: None },
    Renamed { mime: b"windows-31j", iconv: Some(b"CP932"), retry: None },
    Renamed { mime: b"iso-8859-8-i", iconv: Some(b"ISO-8859-8"), retry: None },
    Renamed { mime: b"iso-2022-jp", iconv: None, retry: Some(b"CP50221") },
    Renamed { mime: b"shift_jis", iconv: None, retry: Some(b"CP932") },
    Renamed { mime: b"euc-jp", iconv: None, retry: Some(b"EUC-JP-MS") },
    Renamed { mime: b"gb2312", iconv: None, retry: Some(b"GB18030") },
    Renamed { mime: b"big5", iconv: None, retry: Some(b"BIG5-HKSCS") },
    Renamed { mime: b"euc-kr", iconv: None, retry: Some(b"CP949") },
    Renamed { mime: b"tis-620", iconv: None, retry: Some(b"CP874") },
];

/// The charset of an encoded-word.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Charset {
    /// For `mewl-rs-compat`: the converters of glibc that mewl tries,
    /// for the name and for the superset to retry with, if known.
    Glibc { name: Option<Converter>, retry: Option<Converter> },
    /// For `mewl-rs`.
    Rfc(Converter),
}

impl Charset {
    /// The charset for a MIME charset name, if known by `rules`.
    pub fn for_name(name: &[u8], rules: Rules) -> Option<Self> {
        match rules {
            Rules::Glibc => {
                let (mut iconv, mut retry) = (name, None);
                if let Some(renamed) = RENAMED.iter().find(|r| r.mime.eq_ignore_ascii_case(name)) {
                    iconv = renamed.iconv.unwrap_or(name);
                    retry = renamed.retry;
                }
                let charset = Charset::Glibc { name: glibc_converter(iconv), retry: retry.and_then(glibc_converter) };
                (charset != Charset::Glibc { name: None, retry: None }).then_some(charset)
            }
            Rules::Rfc => {
                if let Some(variant) = iso2022jp::Variant::for_charset(name) {
                    return Some(Charset::Rfc(Converter::Iso2022Jp(variant)));
                }
                if let Some(variant) = utf7::Variant::for_charset(name) {
                    return Some(Charset::Rfc(Converter::Utf7(variant)));
                }
                if let Some(encoding) = Encoding::for_label_no_replacement(name) {
                    return Some(Charset::Rfc(Converter::Encoding(encoding)));
                }
                glibc_converter(name).map(Charset::Rfc)
            }
        }
    }

    /// Whether adjacent encoded-words in this charset may be converted
    /// together. Not for a stateful charset whose encoded-word begins
    /// and ends in its initial state, ISO-2022-JP and UTF-7: joined, a
    /// shift right after another one would be an error.
    pub fn joinable(self) -> bool {
        !matches!(self, Charset::Rfc(Converter::Iso2022Jp(_) | Converter::Utf7(_)))
    }

    /// Converts `input` to UTF-8; `None` if it fails. As mewl does,
    /// [`Charset::Glibc`] tries the name, the superset, the superset
    /// keeping what is invalid, and the name keeping what is invalid.
    /// [`Charset::Rfc`] replaces what is invalid with U+FFFD.
    pub fn convert(self, input: &[u8]) -> Option<Vec<u8>> {
        match self {
            Charset::Glibc { name, retry } => {
                let by = |converter: Option<Converter>, keep| converter?.convert(input, Rules::Glibc, keep);
                by(name, false).or_else(|| by(retry, false)).or_else(|| by(retry, true)).or_else(|| by(name, true))
            }
            Charset::Rfc(converter) => {
                converter.convert(input, Rules::Rfc, false).or_else(|| converter.convert(input, Rules::Rfc, true))
            }
        }
    }
}

/// The converter emulated for a charset name known to glibc.
fn glibc_converter(name: &[u8]) -> Option<Converter> {
    let name = normalize(name);
    let i = glibc::NAMES.binary_search_by(|(known, _)| known.cmp(&name.as_slice())).ok()?;
    Some(glibc::NAMES[i].1)
}

/// `name` as glibc compares it: in upper case, without the characters
/// other than letters, digits, and `_-.,:` (iconv/gconv_charset.c).
fn normalize(name: &[u8]) -> Vec<u8> {
    name.iter().filter(|c| c.is_ascii_alphanumeric() || b"_-.,:".contains(c)).map(u8::to_ascii_uppercase).collect()
}

/// Converts `input` to UTF-8 by encoding_rs. With `keep_invalid`, an
/// invalid byte sequence is kept as it is; otherwise it fails.
fn convert(encoding: &'static Encoding, input: &[u8], keep_invalid: bool) -> Option<Vec<u8>> {
    let mut decoder = encoding.new_decoder_without_bom_handling();
    let size = decoder.max_utf8_buffer_length_without_replacement(input.len())?;
    let mut buf = vec![0; size];
    let mut out = Vec::with_capacity(size);
    let mut src = input;
    loop {
        let (result, read, written) = decoder.decode_to_utf8_without_replacement(src, &mut buf, true);
        out.extend_from_slice(&buf[..written]);
        match result {
            DecoderResult::InputEmpty => return Some(out),
            DecoderResult::OutputFull => unreachable!("the buffer is large enough"),
            DecoderResult::Malformed(bad, extra) => {
                if !keep_invalid {
                    return None;
                }
                let end = read.saturating_sub(extra.into());
                let beg = end.saturating_sub(bad.into());
                out.extend_from_slice(&src[beg..end]);
                src = &src[read..];
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn glibc(name: &str) -> Option<Charset> {
        Charset::for_name(name.as_bytes(), Rules::Glibc)
    }

    fn rfc(name: &str) -> Option<Charset> {
        Charset::for_name(name.as_bytes(), Rules::Rfc)
    }

    #[test]
    fn names_sorted() {
        assert!(glibc::NAMES.windows(2).all(|w| w[0].0 < w[1].0));
        assert!(glibc::NAMES.iter().all(|(name, _)| normalize(name) == *name));
    }

    #[test]
    fn glibc_names() {
        let sjis = Some(Converter::Encoding(encoding_rs::SHIFT_JIS));
        /* as glibc normalizes a name */
        for name in ["SJIS", "sjis", "Shift_JIS", "shift-jis", "MS_KANJI", "s!jis", "S~J+I'S"] {
            assert_eq!(glibc(name).map(|c| matches!(c, Charset::Glibc { name, .. } if name == sjis)), Some(true), "{name}");
        }
        for name in ["shiftjis", "x-sjis", "csiso88598i", "x-mac-roman", "koi", "unicode-1-1-utf-8", "iso-2022-jp-1", "UTF-32", "IBM037"] {
            assert_eq!(glibc(name), None, "{name}");
        }
        /* renamed or retried as mewl does */
        let cp949 = Some(Converter::Encoding(encoding_rs::EUC_KR));
        assert_eq!(glibc("KS_C_5601-1987"), Some(Charset::Glibc { name: cp949, retry: None }));
        assert_eq!(glibc("euc-kr"), Some(Charset::Glibc { name: cp949, retry: cp949 }));
        assert_eq!(glibc("Shift_JIS"), Some(Charset::Glibc { name: sjis, retry: sjis }));
        let jp = Some(Converter::Iso2022Jp(iso2022jp::Variant::Jp));
        /* CP50221 is unknown to glibc */
        assert_eq!(glibc("iso-2022-jp"), Some(Charset::Glibc { name: jp, retry: None }));
        assert_eq!(glibc("iso-2022-jp-2"), Some(Charset::Glibc { name: Some(Converter::Iso2022Jp(iso2022jp::Variant::Jp2)), retry: None }));
    }

    #[test]
    fn rfc_names() {
        /* the Encoding Standard first */
        assert_eq!(rfc("latin1"), Some(Charset::Rfc(Converter::Encoding(encoding_rs::WINDOWS_1252))));
        assert_eq!(rfc("x-mac-roman"), Some(Charset::Rfc(Converter::Encoding(encoding_rs::MACINTOSH))));
        assert_eq!(rfc("iso-2022-jp-1"), Some(Charset::Rfc(Converter::Iso2022Jp(iso2022jp::Variant::Jp1))));
        /* then glibc */
        for (name, encoding) in [
            ("cp949", encoding_rs::EUC_KR),
            ("UHC", encoding_rs::EUC_KR),
            ("cp936", encoding_rs::GBK),
            ("cp950", encoding_rs::BIG5),
            ("cp874", encoding_rs::WINDOWS_874),
            ("TIS620", encoding_rs::WINDOWS_874),
            ("latin9", encoding_rs::ISO_8859_15),
            ("ANSI_X3.4-1968", encoding_rs::WINDOWS_1252),
            ("euc-jp-ms", encoding_rs::EUC_JP),
            ("ms_kanji", encoding_rs::SHIFT_JIS),
        ] {
            assert_eq!(rfc(name), Some(Charset::Rfc(Converter::Encoding(encoding))), "{name}");
        }
        assert_eq!(rfc("UTF-32"), None);
    }

    #[test]
    fn retried() {
        /* not Shift_JIS, but CP932 by glibc */
        let charset = Charset::Glibc { name: None, retry: Some(Converter::Encoding(encoding_rs::SHIFT_JIS)) };
        assert_eq!(charset.convert(b"\x87\x40"), Some("①".into()));
        assert_eq!(Charset::Glibc { name: None, retry: None }.convert(b"a"), None);
    }
}
