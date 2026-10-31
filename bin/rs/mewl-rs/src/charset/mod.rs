//! Charsets of encoded-words and their conversion to UTF-8.
//!
//! mewl converts a charset with iconv: it renames a few MIME charset
//! names for iconv and, for a few others, retries with a superset when
//! the conversion fails (see [`RENAMED`]). `mewl-rs-compat` does the
//! same, knowing a name as glibc does: [`glibc::NAMES`] lists the names
//! glibc knows for each converter emulated. A single-byte charset is
//! converted by the table of glibc, Shift_JIS and EUC-JP by
//! [`crate::jis`], and the others by encoding_rs or a decoder of
//! mewl-rs.
//!
//! `mewl-rs` knows the names of the Encoding Standard and, for the
//! converters emulated, those of glibc as well, and converts a charset
//! as Mew does, by Emacs, as far as the characters have Unicode, and
//! otherwise as the Encoding Standard does: ISO-8859-1, ISO-8859-9,
//! ISO-8859-11, and TIS-620 by the tables of glibc, not as supersets of
//! Windows; Shift_JIS and EUC-JP, told from CP932 and eucJP-ms as glibc
//! does, with seven characters of JIS X 0208 as Emacs takes them
//! ([`crate::jis`]); GB 2312, GB18030, EUC-KR, and Big5 with a few
//! characters as Emacs takes them ([`patched`]); and the rest by
//! encoding_rs. Where Mew guesses, so does `mewl-rs` ([`crate::guess`]):
//! a charset not known, US-ASCII with an 8-bit byte, and UTF-8 with an
//! 8-bit byte between ASCII ones.

pub mod glibc;
pub mod patched;

use encoding_rs::{BIG5, DecoderResult, EUC_JP, EUC_KR, Encoding, GB18030, GBK, SHIFT_JIS, UTF_8};

use crate::guess;
use crate::iso2022jp;
use crate::jis;
use crate::mime::Rules;
use crate::utf7;
use patched::Patch;

/// A converter of a charset to UTF-8.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Converter {
    /// By encoding_rs.
    Encoding(&'static Encoding),
    /// ISO-2022-JP and its variants.
    Iso2022Jp(iso2022jp::Variant),
    /// UTF-7 or UTF-7-IMAP, unknown to the Encoding Standard.
    Utf7(utf7::Variant),
    /// A single-byte charset of glibc, for [`Rules::Glibc`].
    Single(&'static SingleByte),
    /// Shift_JIS, EUC-JP, or a variant.
    Jis(jis::Variant),
    /// GB 2312, GB18030, EUC-KR, or Big5: by encoding_rs, and for
    /// [`Rules::Rfc`] with a few characters as Emacs takes them.
    Patched(Patch),
    /// Guessing the charset, for [`Rules::Rfc`] ([`crate::guess`]).
    Guess,
}

/// A single-byte charset of glibc whose lower half is US-ASCII.
#[derive(Debug, PartialEq, Eq)]
pub struct SingleByte {
    /// The encoding of encoding_rs for the charset, for [`Rules::Rfc`].
    encoding: &'static Encoding,
    /// Whether [`Rules::Rfc`] takes the table instead, as Mew does, the
    /// encoding being a superset of Windows.
    mew: bool,
    /// The code points of 0x80 to 0xFF, 0 if invalid.
    upper: [u16; 128],
}

impl SingleByte {
    /// ISO-8859-1 without the C1 controls (0x80 to 0x9F).
    pub(crate) const fn latin1_without_c1() -> Self {
        let mut upper = [0; 128];
        let mut i = 0x20;
        while i < 128 {
            upper[i] = 0x80 + i as u16;
            i += 1;
        }
        SingleByte { encoding: encoding_rs::WINDOWS_1252, mew: true, upper }
    }

    /// Converts `input` to UTF-8. With `keep_invalid`, an invalid byte is
    /// kept as it is by [`Rules::Glibc`] and replaced with U+FFFD by
    /// [`Rules::Rfc`]; otherwise it fails.
    fn convert(&self, input: &[u8], rules: Rules, keep_invalid: bool) -> Option<Vec<u8>> {
        let mut out = Vec::with_capacity(input.len() * 2);
        for &b in input {
            if b < 0x80 {
                out.push(b);
                continue;
            }
            match char::from_u32(self.upper[usize::from(b - 0x80)].into()).filter(|&c| c != '\0') {
                Some(c) => out.extend_from_slice(c.encode_utf8(&mut [0; 4]).as_bytes()),
                None if keep_invalid && rules == Rules::Rfc => out.extend_from_slice("\u{fffd}".as_bytes()),
                None if keep_invalid => out.push(b),
                None => return None,
            }
        }
        Some(out)
    }
}

impl Converter {
    /// Converts `input` to UTF-8 by `rules`. With `keep_invalid`, an
    /// invalid byte sequence is kept as it is by [`Rules::Glibc`], as
    /// mewl does, and replaced with U+FFFD by [`Rules::Rfc`] but an
    /// invalid sequence of UTF-7 in US-ASCII; otherwise it fails.
    pub(crate) fn convert(self, input: &[u8], rules: Rules, keep_invalid: bool) -> Option<Vec<u8>> {
        match self {
            Converter::Guess => Some(guess::guess(input)),
            Converter::Encoding(encoding) if keep_invalid && rules == Rules::Rfc => {
                let (text, _) = encoding.decode_without_bom_handling(input);
                Some(text.into_owned().into_bytes())
            }
            Converter::Encoding(encoding) => convert(encoding, input, keep_invalid),
            Converter::Iso2022Jp(variant) => iso2022jp::convert(variant, rules, input, keep_invalid),
            Converter::Utf7(variant) => utf7::convert(variant, rules, input, keep_invalid),
            Converter::Single(table) => table.convert(input, rules, keep_invalid),
            Converter::Jis(variant) => jis::convert(variant, rules, input, keep_invalid),
            Converter::Patched(patch) if rules == Rules::Rfc => patched::convert(patch, input, keep_invalid),
            Converter::Patched(patch) => convert(patch.encoding(), input, keep_invalid),
        }
    }

    /// What `mewl-rs` takes a converter of glibc for. US-ASCII, whose
    /// upper half is invalid, is guessed when it is not.
    fn for_rfc(self) -> Self {
        match self {
            Converter::Single(table) if table.upper.iter().all(|&c| c == 0) => Converter::Guess,
            Converter::Single(table) if !table.mew => Converter::Encoding(table.encoding),
            converter => converter,
        }
    }

    /// What `mewl-rs` takes an encoding of encoding_rs named `name` for:
    /// Shift_JIS, EUC-JP, GB 2312 (but GBK), GB18030, EUC-KR, and Big5
    /// (but Big5-HKSCS) with a few characters as Emacs takes them.
    fn for_encoding(name: &[u8], encoding: &'static Encoding) -> Self {
        let named = |s: &[u8]| name.windows(s.len()).any(|w| w.eq_ignore_ascii_case(s));
        match encoding {
            e if e == SHIFT_JIS => Converter::Jis(jis::Variant::ShiftJis),
            e if e == EUC_JP => Converter::Jis(jis::Variant::EucJp),
            e if e == GBK && !named(b"gbk") => Converter::Patched(Patch::Gb2312),
            e if e == GB18030 => Converter::Patched(Patch::Gb18030),
            e if e == EUC_KR => Converter::Patched(Patch::EucKr),
            e if e == BIG5 && !named(b"hkscs") => Converter::Patched(Patch::Big5),
            e => Converter::Encoding(e),
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
                if name.eq_ignore_ascii_case(b"unicode-1-1-utf-7") {
                    /* the name of RFC 1642, which Mew knows */
                    return Some(Charset::Rfc(Converter::Utf7(utf7::Variant::Utf7)));
                }
                if [&b"gb2312"[..], b"cn-gb"].iter().any(|n| name.eq_ignore_ascii_case(n)) {
                    /* Mew decodes them as GBK, which they often are. */
                    return Some(Charset::Rfc(Converter::Encoding(GBK)));
                }
                /* glibc tells apart what the Encoding Standard does not,
                such as Shift_JIS and CP932, or ISO-8859-1 and windows-1252. */
                let converter = glibc_converter(name).map(Converter::for_rfc);
                if let Some(Converter::Jis(_) | Converter::Patched(_) | Converter::Single(_) | Converter::Guess) =
                    converter
                {
                    return converter.map(Charset::Rfc);
                }
                if let Some(encoding) = Encoding::for_label_no_replacement(name) {
                    return Some(Charset::Rfc(Converter::for_encoding(name, encoding)));
                }
                /* A charset not known is guessed, as Mew does. */
                Some(Charset::Rfc(converter.unwrap_or(Converter::Guess)))
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
            /* UTF-8 with an 8-bit byte between ASCII is guessed, as Mew does. */
            Charset::Rfc(Converter::Encoding(e)) if e == UTF_8 && guess::lone_8bit(input) => {
                Converter::Guess.convert(input, Rules::Rfc, true)
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

    fn decode(name: &str, input: &[u8]) -> Option<String> {
        Charset::for_name(name.as_bytes(), Rules::Glibc)?.convert(input).map(|s| String::from_utf8_lossy(&s).into_owned())
    }

    #[test]
    fn single_byte() {
        /* Latin-1 and US-ASCII are not windows-1252. */
        assert_eq!(decode("ISO-8859-1", b"caf\xe9 \x80\x9f"), Some("café \u{80}\u{9f}".into()));
        assert_eq!(decode("latin1", b"\xff"), Some("ÿ".into()));
        /* also for mewl-rs, as Mew does, but what is invalid is U+FFFD */
        assert_eq!(rfc("latin1").unwrap().convert(b"\x80\xe9"), Some("\u{80}é".into()));
        assert_eq!(rfc("iso-8859-9").unwrap().convert(b"\x80\xd0"), Some("\u{80}Ğ".into()));
        assert_eq!(rfc("tis-620").unwrap().convert(b"\xa1\x80"), Some("ก\u{fffd}".into()));
        assert_eq!(rfc("windows-1252").unwrap().convert(b"\x80"), Some("€".into()));
        assert_eq!(decode("US-ASCII", b"a\xe9b"), Some("a\u{fffd}b".into()));
        assert_eq!(Charset::for_name(b"us-ascii", Rules::Glibc).unwrap().convert(b"a\xe9b"), Some(b"a\xe9b".to_vec()));
        /* ISO-8859-9 is not windows-1254, nor TIS-620 windows-874 */
        assert_eq!(decode("ISO-8859-9", b"\x80\xd0"), Some("\u{80}Ğ".into()));
        assert_eq!(decode("TIS-620", b"\xa1"), Some("ก".into()));
        assert_eq!(Charset::for_name(b"TIS620", Rules::Glibc).unwrap().convert(b"\x80"), Some(b"\x80".to_vec()));
        /* but tis-620, in any case, is retried with CP874 */
        assert_eq!(decode("TIS-620", b"\x80"), Some("€".into()));
        /* An undefined byte of windows-125x is invalid. */
        assert_eq!(Charset::for_name(b"windows-1252", Rules::Glibc).unwrap().convert(b"\x81\x80"), Some(b"\x81\xe2\x82\xac".to_vec()));
    }

    #[test]
    fn glibc_names() {
        let sjis = Some(Converter::Jis(jis::Variant::ShiftJis));
        /* as glibc normalizes a name */
        for name in ["SJIS", "sjis", "Shift_JIS", "shift-jis", "MS_KANJI", "s!jis", "S~J+I'S"] {
            assert_eq!(glibc(name).map(|c| matches!(c, Charset::Glibc { name, .. } if name == sjis)), Some(true), "{name}");
        }
        for name in ["shiftjis", "x-sjis", "csiso88598i", "x-mac-roman", "koi", "unicode-1-1-utf-8", "iso-2022-jp-1", "UTF-32", "IBM037"] {
            assert_eq!(glibc(name), None, "{name}");
        }
        /* renamed or retried as mewl does */
        let cp949 = Some(Converter::Patched(Patch::EucKr));
        assert_eq!(glibc("KS_C_5601-1987"), Some(Charset::Glibc { name: cp949, retry: None }));
        assert_eq!(glibc("euc-kr"), Some(Charset::Glibc { name: cp949, retry: cp949 }));
        let cp932 = Some(Converter::Jis(jis::Variant::Cp932));
        assert_eq!(glibc("Shift_JIS"), Some(Charset::Glibc { name: sjis, retry: cp932 }));
        let jp = Some(Converter::Iso2022Jp(iso2022jp::Variant::Jp));
        /* CP50221 is unknown to glibc */
        assert_eq!(glibc("iso-2022-jp"), Some(Charset::Glibc { name: jp, retry: None }));
        assert_eq!(glibc("iso-2022-jp-2"), Some(Charset::Glibc { name: Some(Converter::Iso2022Jp(iso2022jp::Variant::Jp2)), retry: None }));
    }

    #[test]
    fn rfc_names() {
        /* the Encoding Standard, but where glibc tells apart more */
        assert_eq!(rfc("x-cp1252"), Some(Charset::Rfc(Converter::Encoding(encoding_rs::WINDOWS_1252))));
        assert!(matches!(rfc("latin1"), Some(Charset::Rfc(Converter::Single(_)))));
        assert_eq!(rfc("x-mac-roman"), Some(Charset::Rfc(Converter::Encoding(encoding_rs::MACINTOSH))));
        assert_eq!(rfc("iso-2022-jp-1"), Some(Charset::Rfc(Converter::Iso2022Jp(iso2022jp::Variant::Jp1))));
        /* then glibc */
        for (name, encoding) in [
            ("cp936", encoding_rs::GBK),
            ("cp874", encoding_rs::WINDOWS_874),
            ("latin9", encoding_rs::ISO_8859_15),
            ("koi-8", encoding_rs::KOI8_R),
        ] {
            assert_eq!(rfc(name), Some(Charset::Rfc(Converter::Encoding(encoding))), "{name}");
        }
        /* guessed, as Mew does */
        assert_eq!(rfc("UTF-32"), Some(Charset::Rfc(Converter::Guess)));
        assert_eq!(rfc("x-unknown"), Some(Charset::Rfc(Converter::Guess)));
        assert_eq!(rfc("us-ascii"), Some(Charset::Rfc(Converter::Guess)));
        assert_eq!(rfc("ANSI_X3.4-1968"), Some(Charset::Rfc(Converter::Guess)));
        assert_eq!(rfc("unicode-1-1-utf-7"), Some(Charset::Rfc(Converter::Utf7(utf7::Variant::Utf7))));
        assert_eq!(glibc("unicode-1-1-utf-7"), None);
        /* with a few characters as Emacs takes them */
        for (name, patch) in [
            ("chinese", Patch::Gb2312),
            ("euc-cn", Patch::Gb2312),
            ("gb18030", Patch::Gb18030),
            ("euc-kr", Patch::EucKr),
            ("ks_c_5601-1987", Patch::EucKr),
            ("cp949", Patch::EucKr),
            ("big5", Patch::Big5),
            ("cp950", Patch::Big5),
            ("x-x-big5", Patch::Big5),
        ] {
            assert_eq!(rfc(name), Some(Charset::Rfc(Converter::Patched(patch))), "{name}");
        }
        /* GBK, as Mew decodes GB2312 */
        for (name, encoding) in [
            ("gbk", encoding_rs::GBK),
            ("x-gbk", encoding_rs::GBK),
            ("gb2312", encoding_rs::GBK),
            ("GB2312", encoding_rs::GBK),
            ("cn-gb", encoding_rs::GBK),
            ("big5-hkscs", encoding_rs::BIG5),
        ] {
            assert_eq!(rfc(name), Some(Charset::Rfc(Converter::Encoding(encoding))), "{name}");
        }
        /* Shift_JIS and EUC-JP, told from CP932 and eucJP-ms */
        for (name, variant) in [
            ("shift_jis", jis::Variant::ShiftJis),
            ("ms_kanji", jis::Variant::ShiftJis),
            ("x-sjis", jis::Variant::ShiftJis),
            ("windows-31j", jis::Variant::Cp932),
            ("cp932", jis::Variant::Cp932),
            ("euc-jp", jis::Variant::EucJp),
            ("x-euc-jp", jis::Variant::EucJp),
            ("euc-jp-ms", jis::Variant::EucJpMs),
        ] {
            assert_eq!(rfc(name), Some(Charset::Rfc(Converter::Jis(variant))), "{name}");
        }
    }

    #[test]
    fn retried() {
        /* not Shift_JIS, but CP932 by glibc */
        assert_eq!(decode("shift_jis", b"\x81\x60\\"), Some("〜¥".into()));
        assert_eq!(decode("shift_jis", b"\x81\x60\\\x87\x40"), Some("～\\①".into()));
        assert_eq!(decode("sjis", b"\x81\x60\\\x87\x40"), Some("〜¥\u{fffd}@".into()));
        assert_eq!(decode("euc-jp", b"\xa1\xc1\xad\xa1"), Some("～①".into()));
        assert_eq!(Charset::Glibc { name: None, retry: None }.convert(b"a"), None);
    }
}
