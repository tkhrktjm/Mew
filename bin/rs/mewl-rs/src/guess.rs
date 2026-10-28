//! Guessing the charset of text whose charset is not told, or is
//! wrong, for `mewl-rs`, as Mew does by Emacs (`undecided`).
//!
//! Mew guesses an encoded-word in a charset it does not know, raw 8-bit
//! text in a header, and an encoded-word in US-ASCII or UTF-8 with an
//! 8-bit byte between ASCII ones ([`lone_8bit`]), and Emacs guesses by
//! the coding systems of the language
//! environment, which follows the locale: in Japanese, UTF-8,
//! ISO-2022-JP, EUC-JP, and Shift_JIS, in this order. So does this, by
//! `LC_ALL`, `LC_CTYPE`, or `LANG`, taking the first that decodes the
//! text without an error, and ISO-8859-1 at last, as Emacs does:
//!
//! - Japanese (`ja`): UTF-8, ISO-2022-JP, EUC-JP, and Shift_JIS.
//! - Korean (`ko`): UTF-8 and EUC-KR.
//! - Chinese (`zh`): UTF-8, and Big5 in Taiwan and Hong Kong (`zh_TW`
//!   and `zh_HK`) or GB 2312 (GBK) elsewhere.
//! - Otherwise: UTF-8, ISO-8859-1 without the C1 controls, which Emacs
//!   takes as unlikely, and EUC-JP and Shift_JIS, which Emacs with Mew
//!   finds in Japanese text in any language environment.
//!
//! Where Emacs leaves bytes undecoded, e.g. an incomplete character at
//! the end, this goes on to the next charset, so that the text can be
//! found. ISO-2022-JP, which Emacs does not decode in a string by
//! guessing, is decoded whatever the locale.

use std::env;
use std::str;
use std::sync::OnceLock;

use crate::charset::{Converter, SingleByte};
use crate::charset::patched::Patch;
use crate::iso2022jp;
use crate::jis;
use crate::mime::Rules;

const ESC: u8 = 0x1b;

/// Decodes `input` into UTF-8 by guessing its charset.
pub fn guess(input: &[u8]) -> Vec<u8> {
    if input.is_ascii() && input.contains(&ESC) {
        return iso2022jp::convert(iso2022jp::Variant::Jp, Rules::Rfc, input, true).unwrap_or_default();
    }
    if str::from_utf8(input).is_ok() {
        return input.to_vec();
    }
    candidates()
        .iter()
        .find_map(|&converter| try_charset(converter, input))
        .unwrap_or_else(|| input.iter().map(|&b| char::from(b)).collect::<String>().into_bytes())
}

/// `input` decoded by `converter` if it is in the charset. Shift_JIS and
/// EUC-JP are taken to have JIS X 0201, 0208, and 0212 only, as Emacs
/// does, without the extensions of Windows and the user-defined area,
/// which random bytes are likely to be.
fn try_charset(converter: Converter, input: &[u8]) -> Option<Vec<u8>> {
    if let Converter::Jis(variant) = converter {
        jis::convert(variant, Rules::Glibc, input, false)?;
    }
    converter.convert(input, Rules::Rfc, false)
}

/// Whether `input` has an 8-bit byte between ASCII ones, or at an end,
/// for which Mew guesses an encoded-word in UTF-8
/// (`mew-regex-singlebyte-nonascii`): one of a broken character of
/// UTF-8 is not.
pub fn lone_8bit(input: &[u8]) -> bool {
    let ascii = |i: Option<usize>| i.and_then(|i| input.get(i)).is_none_or(u8::is_ascii);
    (0..input.len()).any(|i| !input[i].is_ascii() && ascii(i.checked_sub(1)) && ascii(Some(i + 1)))
}

/// The charsets but UTF-8 to try by the locale.
fn candidates() -> &'static [Converter] {
    static CANDIDATES: OnceLock<Vec<Converter>> = OnceLock::new();
    CANDIDATES.get_or_init(|| {
        let locale = ["LC_ALL", "LC_CTYPE", "LANG"]
            .iter()
            .find_map(|name| env::var(name).ok().filter(|v| !v.is_empty()))
            .unwrap_or_default();
        candidates_for(&locale)
    })
}

fn candidates_for(locale: &str) -> Vec<Converter> {
    let lang = locale.split(['.', '@']).next().unwrap_or_default();
    let japanese = [Converter::Jis(jis::Variant::EucJp), Converter::Jis(jis::Variant::ShiftJis)];
    match lang.split('_').next().unwrap_or_default() {
        "ja" => japanese.to_vec(),
        "ko" => vec![Converter::Patched(Patch::EucKr)],
        "zh" if matches!(lang, "zh_TW" | "zh_HK") => vec![Converter::Patched(Patch::Big5)],
        "zh" => vec![Converter::Patched(Patch::Gb2312)],
        _ => [&[Converter::Single(&LATIN1_WITHOUT_C1)][..], &japanese].concat(),
    }
}

/// ISO-8859-1 without the C1 controls, as a candidate.
static LATIN1_WITHOUT_C1: SingleByte = SingleByte::latin1_without_c1();

#[cfg(test)]
mod tests {
    use super::*;

    fn guess_in(locale: &str, input: &[u8]) -> String {
        let text = if str::from_utf8(input).is_ok() {
            guess(input)
        } else {
            candidates_for(locale)
                .iter()
                .find_map(|&converter| try_charset(converter, input))
                .unwrap_or_else(|| input.iter().map(|&b| char::from(b)).collect::<String>().into_bytes())
        };
        String::from_utf8(text).unwrap()
    }

    #[test]
    fn japanese() {
        for locale in ["ja_JP.UTF-8", "ja_JP.eucJP", "ja"] {
            assert_eq!(guess_in(locale, "日本".as_bytes()), "日本");
            assert_eq!(guess_in(locale, b"\xc6\xfc\xcb\xdc\xa1\xc1"), "日本〜");
            assert_eq!(guess_in(locale, b"\x93\xfa\x96\x7b\x81\x60"), "日本〜");
            assert_eq!(guess_in(locale, b"\x1b$BF|K\\\x1b(B"), "日本");
            /* EUC-JP first, as Emacs */
            assert_eq!(guess_in(locale, b"\xa1\xa1"), "\u{3000}");
            /* neither: ISO-8859-1 */
            assert_eq!(guess_in(locale, b"\x93hi\x94"), "\u{93}hi\u{94}");
            /* the user-defined area is not taken for Shift_JIS */
            assert_eq!(guess_in(locale, b"\xf0\x42"), "ðB");
            /* but the NEC special characters are not either */
            assert_eq!(guess_in(locale, b"\x87\x40"), "\u{87}@");
        }
    }

    #[test]
    fn lone() {
        assert!(lone_8bit(b"caf\xe9"));
        assert!(lone_8bit(b"\xffab"));
        assert!(lone_8bit(b"a\x93b\x94"));
        assert!(!lone_8bit("日".as_bytes()));
        assert!(!lone_8bit(b"\xe6\x97\xa5\xe6"));
        assert!(!lone_8bit(b"abc"));
    }

    #[test]
    fn others() {
        assert_eq!(guess_in("ko_KR.UTF-8", b"\xc7\xd1"), "한");
        assert_eq!(guess_in("zh_CN.UTF-8", b"\xd6\xd0"), "中");
        assert_eq!(guess_in("zh_TW.UTF-8", b"\xa4\xa4"), "中");
        for locale in ["C.UTF-8", "en_US.UTF-8", ""] {
            assert_eq!(guess_in(locale, b"caf\xe9"), "café");
            assert_eq!(guess_in(locale, b"Stra\xdfe"), "Straße");
            /* EUC-JP without C1 bytes is taken for ISO-8859-1 */
            assert_eq!(guess_in(locale, b"\xc6\xfc"), "Æü");
            /* but Shift_JIS has some */
            assert_eq!(guess_in(locale, b"\x93\xfa\x96\x7b"), "日本");
            assert_eq!(guess_in(locale, b"\x8e\xb1\xa4\xa2"), "ｱあ");
            assert_eq!(guess_in(locale, b"\x93hi\x94"), "\u{93}hi\u{94}");
        }
    }
}
