//! Decoding RFC 2047 encoded-words into UTF-8.
//!
//! [`decode_field`] follows `decode_field()` of mewl: only a word
//! delimited by white space can be an encoded-word, white space between
//! encoded-words is removed, line breaks in white space are removed
//! (unfolding), each encoded-word is converted by itself, and control
//! characters but TAB are removed from what is decoded. A word that is
//! not a valid encoded-word, or in an unknown charset, is kept as is.
//!
//! mewl converts charsets with iconv while this uses encoding_rs (the
//! Encoding Standard), so a few characters may be converted
//! differently, e.g. 0x8160 of Shift_JIS is U+301C by glibc but U+FF5E
//! here.
//!
//! [`decode_text`] is for `mewl-rs` (not `mewl-rs-compat`) and more
//! lenient:
//!
//! - An encoded-word is found anywhere, e.g. in `Re:=?UTF-8?B?...?=`.
//! - Adjacent encoded-words in the same charset are converted together
//!   since some mailers split a multibyte character between them. This
//!   is not done for ISO-2022-JP, whose encoded-word begins and ends in
//!   ASCII; joined, an escape sequence right after another one would
//!   be an error.
//! - Raw ISO-2022-JP, which old Japanese mailers put in headers without
//!   encoding, is converted.
//!
//! Otherwise it is the same as [`decode_field`].

use encoding_rs::{DecoderResult, Encoding, ISO_2022_JP, SHIFT_JIS};

const ESC: u8 = 0x1b;
const DEL: u8 = 0x7f;

/// The max length of a charset name.
const CHARSET_MAX: usize = 64;

fn is_lwsp(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\r' | b'\n')
}

/// Decodes encoded-words in a field body.
pub fn decode_field(s: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len());
    let mut after_word = false;
    let mut p = 0;
    while p < s.len() {
        let q = p + s[p..].iter().take_while(|&&c| is_lwsp(c)).count();
        let e = q + s[q..].iter().take_while(|&&c| !is_lwsp(c)).count();
        let word = &s[q..e];
        /* White space between encoded-words is removed. */
        if after_word && !word.is_empty() && decode_word(word, &mut out) {
            p = e;
            continue;
        }
        /* Unfolding. */
        out.extend(s[p..q].iter().filter(|&&c| c != b'\r' && c != b'\n'));
        if !after_word && !word.is_empty() && decode_word(word, &mut out) {
            after_word = true;
        } else {
            out.extend_from_slice(word);
            after_word = false;
        }
        p = e;
    }
    out
}

/// Decodes encoded-words anywhere in a header value (see above).
pub fn decode_text(s: &[u8]) -> Vec<u8> {
    if s.contains(&ESC) && s.is_ascii() {
        let (text, _) = ISO_2022_JP.decode_without_bom_handling(s);
        decode_anywhere(text.as_bytes())
    } else {
        decode_anywhere(s)
    }
}

/// Decodes each field of a header with [`decode_text`], keeping the
/// line break between fields.
pub fn decode_header(header: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(header.len());
    let mut beg = 0;
    /* A field ends at a line break not followed by white space. */
    let field_ends = header
        .iter()
        .enumerate()
        .filter(|&(i, &c)| c == b'\n' && !matches!(header.get(i + 1), Some(b' ' | b'\t')));
    for (end, _) in field_ends {
        out.extend(decode_text(&header[beg..end]));
        out.push(b'\n');
        beg = end + 1;
    }
    if beg < header.len() {
        out.extend(decode_text(&header[beg..]));
    }
    out
}

fn decode_anywhere(s: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len());
    /* Raw bytes of adjacent encoded-words in the same charset. */
    let mut pending: Option<Word> = None;
    let mut after_word = false;
    let mut text_start = 0;
    let mut i = 0;

    while i < s.len() {
        let Some((len, word)) = find_word(&s[i..]) else {
            i += 1;
            continue;
        };
        let between = &s[text_start..i];
        /* White space between encoded-words is removed. */
        if !(after_word && between.iter().all(|&c| is_lwsp(c))) {
            flush(&mut out, pending.take());
            /* Unfolding. */
            out.extend(between.iter().filter(|&&c| c != b'\r' && c != b'\n'));
        }
        match &mut pending {
            Some(p) if p.encoding == word.encoding && word.encoding != ISO_2022_JP => p.bytes.extend(word.bytes),
            _ => {
                flush(&mut out, pending.take());
                pending = Some(word);
            }
        }
        i += len;
        text_start = i;
        after_word = true;
    }
    flush(&mut out, pending.take());
    out.extend(s[text_start..].iter().filter(|&&c| c != b'\r' && c != b'\n'));
    out
}

fn flush(out: &mut Vec<u8>, word: Option<Word>) {
    if let Some(word) = word {
        word.convert_into(out);
    }
}

/// An encoded-word at the beginning of `s` and its length.
fn find_word(s: &[u8]) -> Option<(usize, Word)> {
    let rest = s.strip_prefix(b"=?")?;
    /* =? charset ? kind ? text ?= where none has "?" or white space */
    let cs_len = rest.iter().position(|&c| c == b'?' || is_lwsp(c))?;
    let rest = rest.get(cs_len..)?.strip_prefix(b"?")?;
    let rest = rest.get(1..)?.strip_prefix(b"?")?;
    let text_len = rest.iter().position(|&c| c == b'?' || is_lwsp(c))?;
    rest.get(text_len..)?.strip_prefix(b"?=")?;
    let len = 2 + cs_len + 3 + text_len + 2;
    Word::parse(&s[..len]).map(|w| (len, w))
}

/// Decodes `=?charset[*lang]?B|Q?encoded-text?=` and appends it to `out`.
/// `false` (and nothing appended) if `word` is not an encoded-word.
fn decode_word(word: &[u8], out: &mut Vec<u8>) -> bool {
    match Word::parse(word) {
        Some(word) => word.convert_into(out),
        None => false,
    }
}

/// An encoded-word decoded into bytes in its charset.
struct Word {
    encoding: &'static Encoding,
    bytes: Vec<u8>,
}

impl Word {
    /// Parses `word`, which is an encoded-word as a whole. `None` if it
    /// is not valid or the charset is unknown.
    fn parse(word: &[u8]) -> Option<Self> {
        let inner = word.strip_prefix(b"=?")?.strip_suffix(b"?=")?;
        if word.len() < 9 {
            return None;
        }
        /* charset */
        let cs_len = inner.iter().position(|&c| c == b'?')?;
        let (charset, rest) = (&inner[..cs_len], &inner[cs_len + 1..]);
        if !charset.iter().all(|&c| is_token(c)) || rest.len() < 3 {
            return None;
        }
        /* ignoring the language of RFC 2231 */
        let charset = charset.split(|&c| c == b'*').next().unwrap_or_default();
        if charset.is_empty() || charset.len() >= CHARSET_MAX {
            return None;
        }
        /* encoding */
        let [kind, b'?', text @ ..] = rest else {
            return None;
        };
        let bytes = match kind.to_ascii_uppercase() {
            b'B' => decode_b(text)?,
            b'Q' => decode_q(text)?,
            _ => return None,
        };
        let encoding = encoding_for(charset)?; /* unknown charset */
        Some(Word { encoding, bytes })
    }

    /// Converts to UTF-8 and appends it to `out`. `false` (and nothing
    /// appended) if it fails.
    fn convert_into(self, out: &mut Vec<u8>) -> bool {
        let converted = convert(self.encoding, &self.bytes, false).or_else(|| convert(self.encoding, &self.bytes, true));
        let Some(text) = converted else {
            return false;
        };
        /* Control characters but TAB are removed as Mew does. */
        out.extend(text.into_iter().filter(|&c| c == b'\t' || (c >= b' ' && c != DEL)));
        true
    }
}

/// A character of `token` of RFC 2047, with `\` of erratum 506.
fn is_token(c: u8) -> bool {
    c > b' ' && c < DEL && !b"()<>@,;:\\\"/[]?.=".contains(&c)
}

/// The encoding for a MIME charset name.
fn encoding_for(charset: &[u8]) -> Option<&'static Encoding> {
    if let Some(encoding) = Encoding::for_label_no_replacement(charset) {
        return Some(encoding);
    }
    /* Names known to iconv but not to the Encoding Standard. */
    match charset.to_ascii_lowercase().as_slice() {
        b"cp932" | b"ms932" => Some(SHIFT_JIS),
        b"iso-2022-jp-1" | b"iso-2022-jp-2" | b"iso-2022-jp-3" => Some(ISO_2022_JP),
        _ => None,
    }
}

/// Converts `input` to UTF-8. With `keep_invalid`, an invalid byte
/// sequence is kept as it is; otherwise it fails.
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

/// The "B" encoding. Missing or extra padding of a buggy client is
/// repaired as Mew does.
fn decode_b(text: &[u8]) -> Option<Vec<u8>> {
    fn value(c: u8) -> Option<u32> {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        };
        Some(v.into())
    }

    let data = &text[..text.len() - text.iter().rev().take_while(|&&c| c == b'=').count()];
    if data.len() % 4 == 1 {
        return None;
    }
    let mut out = Vec::with_capacity(data.len() * 3 / 4);
    for chunk in data.chunks(4) {
        let mut n = 0;
        for (k, &c) in chunk.iter().enumerate() {
            n |= value(c)? << (18 - 6 * k);
        }
        out.extend_from_slice(&n.to_be_bytes()[1..chunk.len()]);
    }
    Some(out)
}

/// The "Q" encoding: `_` is a space and `=XX` is a byte.
fn decode_q(text: &[u8]) -> Option<Vec<u8>> {
    let hex = |c: u8| (c as char).to_digit(16);
    let mut out = Vec::with_capacity(text.len());
    let mut i = 0;
    while i < text.len() {
        match text[i] {
            b'=' => {
                let hi = text.get(i + 1).copied().and_then(hex)?;
                let lo = text.get(i + 2).copied().and_then(hex)?;
                out.push((hi << 4 | lo) as u8);
                i += 3;
                continue;
            }
            b'_' => out.push(b' '),
            c if c > b' ' && c < DEL && c != b'?' => out.push(c),
            _ => return None, /* not in encoded-text */
        }
        i += 1;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode(s: &str) -> String {
        String::from_utf8(decode_field(s.as_bytes())).unwrap()
    }

    #[test]
    fn charsets_and_encodings() {
        assert_eq!(decode("=?UTF-8?B?5pel5pys6Kqe?="), "日本語");
        assert_eq!(decode("=?utf-8?b?5pel5pys6Kqe?="), "日本語");
        assert_eq!(decode("=?ISO-8859-1?Q?caf=E9_au_lait?="), "café au lait");
        assert_eq!(decode("=?ISO-2022-JP?B?GyRCRnxLXDhsGyhC?="), "日本語");
        assert_eq!(decode("=?Shift_JIS?B?k/qWe4zq?="), "日本語");
        assert_eq!(decode("=?EUC-JP?B?xvzL3Ljs?="), "日本語");
        assert_eq!(decode("=?UTF-8*ja?B?5pel5pys6Kqe?="), "日本語");
        /* padding repaired */
        assert_eq!(decode("=?UTF-8?B?5pel5pys6Kqe44GuNA?="), "日本語の4");
        assert_eq!(decode("=?UTF-8?B?5pel5pys6Kqe44GuNA===?="), "日本語の4");
    }

    #[test]
    fn white_space() {
        /* White space and folding between encoded-words is removed. */
        assert_eq!(decode("=?UTF-8?B?5pel?= =?UTF-8?B?5pys?=\n\t=?UTF-8?B?6Kqe?="), "日本語");
        /* but not between an encoded-word and text, where it is unfolded. */
        assert_eq!(decode("Re: =?UTF-8?B?5pel5pys6Kqe?=\n (1)"), "Re: 日本語 (1)");
        assert_eq!(decode(" a\n b"), " a b");
        /* ISO-2022-JP words are converted one by one. */
        assert_eq!(decode("=?ISO-2022-JP?B?GyRCRnxLXDhsGyhC?=\n =?ISO-2022-JP?B?GyRCJE5GfEtcGyhC?="), "日本語の日本");
    }

    #[test]
    fn not_encoded_words() {
        /* An encoded-word has to be a word by itself. */
        assert_eq!(decode("Re:=?UTF-8?B?5pel?="), "Re:=?UTF-8?B?5pel?=");
        for s in ["=?", "=?UTF-8?B?", "=?UTF-8?X?abc?=", "=?UNKNOWN?B?YQ==?=", "=?UTF-8?B?Y?=", "=?UTF-8?Q?a=ZZ?=", "=?UTF-8?Q?a?b?=", "=?U.8?B?YWJj?=", "=?*ja?B?YWJj?="] {
            assert_eq!(decode(s), s);
        }
    }

    #[test]
    fn invalid_and_control() {
        /* An invalid sequence is kept. */
        assert_eq!(decode_field(b"=?UTF-8?B?/2Fi?="), b"\xffab");
        /* Control characters but TAB are removed. */
        assert_eq!(decode("=?UTF-8?Q?a=0D=0A=09b=00?="), "a\tb");
    }

    fn decode_utf8(s: &str) -> String {
        String::from_utf8(decode_text(s.as_bytes())).unwrap()
    }

    #[test]
    fn lenient() {
        /* An encoded-word anywhere. */
        assert_eq!(decode_utf8("Re:=?UTF-8?B?5pel5pys6Kqe?=(2)"), "Re:日本語(2)");
        /* A character split between encoded-words. */
        assert_eq!(decode_utf8("=?UTF-8?B?5pel5g?= =?UTF-8?B?nKzoqp4?="), "日本語");
        assert_eq!(decode_utf8("=?Shift_JIS?B?k/qW?= =?Shift_JIS?B?e4zq?="), "日本語");
        /* ISO-2022-JP words are converted one by one. */
        assert_eq!(decode_utf8("=?ISO-2022-JP?B?GyRCRnxLXDhsGyhC?=\n =?ISO-2022-JP?B?GyRCJE5GfEtcGyhC?="), "日本語の日本");
        /* The rest is as decode_field(). */
        assert_eq!(decode_utf8("Re: =?UTF-8?B?5pel?=\n (1)"), "Re: 日 (1)");
        assert_eq!(decode_utf8("=?UTF-8?Q?a=0D=0Ab?= =?UNKNOWN?B?YQ==?="), "ab =?UNKNOWN?B?YQ==?=");
        assert_eq!(decode_utf8("=?UTF-8?B?a b?="), "=?UTF-8?B?a b?=");
    }

    #[test]
    fn header_field_by_field() {
        let header = "Subject: =?UTF-8?B?5pel?=\n =?UTF-8?B?5pys?=\nFrom: =?UTF-8?B?6Kqe?=\n";
        assert_eq!(String::from_utf8(decode_header(header.as_bytes())).unwrap(), "Subject: 日本\nFrom: 語\n");
    }

    #[test]
    fn raw_iso_2022_jp() {
        assert_eq!(String::from_utf8(decode_text(b"\x1b$BF|K\\8l\x1b(B =?UTF-8?B?5pel?=")).unwrap(), "日本語 日");
    }
}
