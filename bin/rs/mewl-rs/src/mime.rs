//! Decoding header values to UTF-8 (RFC 2047).
//!
//! Encoded-words (`=?charset?B?...?=` and `=?charset?Q?...?=`) are
//! decoded and converted to UTF-8. Following RFC 2047, white space
//! between adjacent encoded-words, including folding, is removed.
//! Adjacent encoded-words in the same charset are converted together
//! since some mailers split a multibyte character between them. This
//! is not done for ISO-2022-JP, whose encoded-word begins and ends in
//! ASCII; joined, an escape sequence right after another one would be
//! an error.
//!
//! Text outside encoded-words is kept as is, except that 7bit text
//! with ISO-2022-JP escape sequences, which old Japanese mailers put
//! in headers without encoding, is converted, too.

use encoding_rs::{Encoding, ISO_2022_JP, SHIFT_JIS};

const ESC: u8 = 0x1b;

/// Decodes a header value. The result is UTF-8 except for bytes which
/// are not in an encoded-word nor ISO-2022-JP.
pub fn decode_header(input: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(input.len());
    /* Raw bytes of adjacent encoded-words in the same charset. */
    let mut pending: Option<(&'static Encoding, Vec<u8>)> = None;
    let mut after_word = false;
    let mut text_start = 0;
    let mut i = 0;

    while i < input.len() {
        let Some(word) = input[i..].starts_with(b"=?").then(|| EncodedWord::parse(&input[i..])).flatten() else {
            i += 1;
            continue;
        };
        let between = &input[text_start..i];
        if !(after_word && between.iter().all(|&c| is_lws(c))) {
            flush(&mut out, &mut pending);
            push_text(&mut out, between);
        }
        match &mut pending {
            Some((encoding, bytes)) if *encoding == word.encoding && word.encoding != ISO_2022_JP => {
                bytes.extend(word.bytes)
            }
            _ => {
                flush(&mut out, &mut pending);
                pending = Some((word.encoding, word.bytes));
            }
        }
        i += word.len;
        text_start = i;
        after_word = true;
    }
    flush(&mut out, &mut pending);
    push_text(&mut out, &input[text_start..]);
    out
}

/// Linear white space, which may separate encoded-words.
fn is_lws(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\r' | b'\n')
}

fn flush(out: &mut Vec<u8>, pending: &mut Option<(&'static Encoding, Vec<u8>)>) {
    if let Some((encoding, bytes)) = pending.take() {
        let (text, _) = encoding.decode_without_bom_handling(&bytes);
        /* A decoded line break would break the output format. */
        out.extend(text.bytes().map(|c| if c == b'\r' || c == b'\n' { b' ' } else { c }));
    }
}

fn push_text(out: &mut Vec<u8>, text: &[u8]) {
    if text.contains(&ESC) && text.is_ascii() {
        let (decoded, _) = ISO_2022_JP.decode_without_bom_handling(text);
        out.extend_from_slice(decoded.as_bytes());
    } else {
        out.extend_from_slice(text);
    }
}

struct EncodedWord {
    encoding: &'static Encoding,
    bytes: Vec<u8>,
    /// The length of `=?...?=`.
    len: usize,
}

impl EncodedWord {
    /// Parses `=?charset[*lang]?B|Q?text?=` at the beginning of `s`.
    /// `None` if it is not an encoded-word or the charset is unknown.
    fn parse(s: &[u8]) -> Option<Self> {
        let rest = s.strip_prefix(b"=?")?;
        let charset_len = rest.iter().position(|&c| c == b'?')?;
        let charset = &rest[..charset_len];
        if charset.is_empty() || charset.iter().any(|&c| c <= b' ' || c >= 0x7f) {
            return None;
        }
        let rest = &rest[charset_len + 1..];
        let (&kind, rest) = rest.split_first()?;
        let rest = rest.strip_prefix(b"?")?;
        let text_len = rest.iter().position(|&c| c == b'?' || is_lws(c))?;
        if rest.get(text_len..text_len + 2) != Some(b"?=") {
            return None;
        }
        let text = &rest[..text_len];
        let bytes = match kind.to_ascii_uppercase() {
            b'B' => decode_base64(text)?,
            b'Q' => decode_q(text),
            _ => return None,
        };
        Some(EncodedWord {
            encoding: encoding_for(charset)?,
            bytes,
            len: 2 + charset_len + 3 + text_len + 2,
        })
    }
}

/// The encoding for a MIME charset name. RFC 2231 language is ignored.
fn encoding_for(charset: &[u8]) -> Option<&'static Encoding> {
    let name = charset.split(|&c| c == b'*').next()?;
    if let Some(encoding) = Encoding::for_label_no_replacement(name) {
        return Some(encoding);
    }
    /* Names used in mail but not in the Encoding Standard. */
    match name.to_ascii_lowercase().as_slice() {
        b"cp932" | b"ms932" | b"x-ms-cp932" => Some(SHIFT_JIS),
        b"iso-2022-jp-1" | b"iso-2022-jp-2" | b"iso-2022-jp-3" | b"iso-2022-jp-2004" => Some(ISO_2022_JP),
        _ => None,
    }
}

/// The Q encoding: `_` is a space and `=XX` is a byte.
fn decode_q(text: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len());
    let mut i = 0;
    while i < text.len() {
        match text[i] {
            b'_' => out.push(b' '),
            b'=' => {
                let hex = text.get(i + 1..i + 3).and_then(|h| std::str::from_utf8(h).ok());
                if let Some(byte) = hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                    out.push(byte);
                    i += 3;
                    continue;
                }
                out.push(b'=');
            }
            c => out.push(c),
        }
        i += 1;
    }
    out
}

/// Base64, allowing missing padding.
fn decode_base64(text: &[u8]) -> Option<Vec<u8>> {
    fn value(c: u8) -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        } as u32)
    }

    let data = match text.iter().position(|&c| c == b'=') {
        Some(pad) if text[pad..].iter().all(|&c| c == b'=') => &text[..pad],
        Some(_) => return None,
        None => text,
    };
    let mut out = Vec::with_capacity(data.len() * 3 / 4);
    for chunk in data.chunks(4) {
        if chunk.len() == 1 {
            return None;
        }
        let mut n = 0;
        for (k, &c) in chunk.iter().enumerate() {
            n |= value(c)? << (18 - 6 * k);
        }
        out.extend_from_slice(&n.to_be_bytes()[1..chunk.len()]);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode(s: &str) -> String {
        String::from_utf8(decode_header(s.as_bytes())).unwrap()
    }

    #[test]
    fn base64_and_q() {
        assert_eq!(decode("=?UTF-8?B?5pel5pys6Kqe?="), "日本語");
        assert_eq!(decode("=?utf-8?b?5pel5pys6Kqe?="), "日本語");
        assert_eq!(decode("=?ISO-8859-1?Q?caf=E9_au_lait?="), "café au lait");
        assert_eq!(decode("=?ISO-2022-JP?B?GyRCRnxLXDhsGyhC?="), "日本語");
        assert_eq!(decode("=?Shift_JIS?B?k/qWe4zq?="), "日本語");
        assert_eq!(decode("=?EUC-JP?B?xvzL3Ljs?="), "日本語");
        assert_eq!(decode("=?UTF-8*ja?B?5pel5pys6Kqe?="), "日本語");
    }

    #[test]
    fn adjacent_words() {
        /* White space and folding between encoded-words is removed. */
        assert_eq!(decode("=?UTF-8?B?5pel?= =?UTF-8?B?5pys?=\n\t=?UTF-8?B?6Kqe?="), "日本語");
        /* but not between an encoded-word and text. */
        assert_eq!(decode("Re: =?UTF-8?B?5pel5pys6Kqe?= (1)"), "Re: 日本語 (1)");
        /* ISO-2022-JP words are converted one by one. */
        assert_eq!(decode("=?ISO-2022-JP?B?GyRCRnxLXDhsGyhC?=\n =?ISO-2022-JP?B?GyRCJE5GfEtcGyhC?="), "日本語の日本");
        /* A character split between encoded-words. */
        assert_eq!(decode("=?UTF-8?B?5pel5g?= =?UTF-8?B?nKzoqp4?="), "日本語");
    }

    #[test]
    fn not_encoded_words() {
        for s in ["=?", "=?UTF-8?B?", "=?UTF-8?X?abc?=", "=?UNKNOWN?B?YQ==?=", "=?UTF-8?B?a b?=", "a=?b", "=?UTF-8?B?Y?="] {
            assert_eq!(decode(s), s);
        }
        assert_eq!(decode("=?UTF-8?Q?a=ZZ?="), "a=ZZ");
    }

    #[test]
    fn raw_iso_2022_jp() {
        assert_eq!(decode("Subject: \x1b$BF|K\\8l\x1b(B!"), "Subject: 日本語!");
    }

    #[test]
    fn line_breaks_in_words() {
        assert_eq!(decode("=?UTF-8?Q?a=0D=0Ab?="), "a  b");
    }
}
