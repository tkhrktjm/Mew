//! UTF-7 (RFC 2152) and UTF-7-IMAP (RFC 3501) to UTF-8, as the iconv
//! of glibc converts them for mewl.
//!
//! The decoder follows what glibc does, so that an invalid sequence is
//! found at the same byte and the state after it is the same:
//!
//! - Outside base64, the direct characters and, for UTF-7, the optional
//!   direct characters but `\` and `~` pass. Any other byte is invalid
//!   by itself. The shift character (`+`, or `&` for IMAP) followed by
//!   `-` is the shift character itself; followed by anything else, it
//!   starts base64. The shift character at the end is incomplete.
//! - In base64, every 16 bits are a UTF-16 unit, and a high surrogate
//!   waits for a low one. The unit after a high surrogate is invalid as
//!   soon as its top bits tell that it is not a low surrogate.
//! - A character not in base64 ends it; it is invalid if 6 bits or
//!   more are pending, if the pending bits are not zero, if a high
//!   surrogate is waiting, or, for IMAP, if it is not `-`. `-` is
//!   consumed; any other character is read again outside base64.
//! - An invalid character does not change the state: glibc resets it
//!   only when invalid characters are ignored (//IGNORE), which mewl
//!   does not ask for. mewl keeps the character and goes on, so after
//!   an invalid character in base64, the rest is still read as base64.
//! - Bits pending at the end are dropped, since mewl does not flush the
//!   conversion.
//!
//! A lone low surrogate differs: glibc passes it to the next step of
//! the conversion, which refuses it in a way that depends on the
//! internals of iconv. Here the character completing it is invalid,
//! like any other invalid character in base64.

/// The two variants iconv knows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Variant {
    Utf7,
    Imap,
}

impl Variant {
    /// The variant for a charset name as iconv takes it.
    pub fn for_charset(charset: &[u8]) -> Option<Self> {
        match charset.to_ascii_lowercase().as_slice() {
            b"utf-7" | b"utf7" => Some(Variant::Utf7),
            b"utf-7-imap" => Some(Variant::Imap),
            _ => None,
        }
    }

    fn shift_char(self) -> u8 {
        match self {
            Variant::Utf7 => b'+',
            Variant::Imap => b'&',
        }
    }

    /// The direct characters, and the optional direct ones for UTF-7.
    fn passes(self, c: u8) -> bool {
        match self {
            Variant::Utf7 => {
                c.is_ascii_alphanumeric()
                    || b"'(),-./:? \t\n\r".contains(&c)
                    || b"!\"#$%&*;<=>@[]^_`{|}".contains(&c)
            }
            Variant::Imap => c != b'&' && (b' '..=b'~').contains(&c),
        }
    }

    fn base64_value(self, c: u8) -> Option<u32> {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' if self == Variant::Utf7 => 63,
            b',' if self == Variant::Imap => 63,
            _ => return None,
        };
        Some(v.into())
    }
}

/// Why decoding stopped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stop {
    /// The byte at this position is invalid.
    Invalid(usize),
    /// The input ends in the middle of a sequence starting here.
    Incomplete(usize),
}

fn is_high(u: u32) -> bool {
    (0xd800..0xdc00).contains(&u)
}

fn is_low(u: u32) -> bool {
    (0xdc00..0xe000).contains(&u)
}

struct Decoder {
    variant: Variant,
    /// 0 outside base64; inside, 32 less the number of pending bits.
    shift: u32,
    /// The pending bits, from the top.
    acc: u32,
}

impl Decoder {
    fn new(variant: Variant) -> Self {
        Decoder { variant, shift: 0, acc: 0 }
    }

    /// Decodes `input` from `pos` into `out` until the end or a stop.
    fn decode(&mut self, input: &[u8], mut pos: usize, out: &mut String) -> Result<(), Stop> {
        while let Some(&c) = input.get(pos) {
            if self.shift == 0 {
                if self.variant.passes(c) {
                    out.push(char::from(c));
                    pos += 1;
                } else if c == self.variant.shift_char() {
                    match input.get(pos + 1) {
                        None => return Err(Stop::Incomplete(pos)),
                        Some(b'-') => {
                            out.push(char::from(c));
                            pos += 2;
                        }
                        Some(_) => {
                            self.shift = 32;
                            self.acc = 0;
                            pos += 1;
                        }
                    }
                } else {
                    return Err(Stop::Invalid(pos));
                }
                continue;
            }

            let Some(i) = self.variant.base64_value(c) else {
                /* The end of base64. */
                if self.acc != 0 || self.shift <= 26 || (self.variant == Variant::Imap && c != b'-') {
                    return Err(Stop::Invalid(pos));
                }
                self.shift = 0;
                if c == b'-' {
                    pos += 1;
                }
                continue;
            };

            if self.shift > 6 {
                let shift = self.shift - 6;
                let acc = self.acc | (i << shift);
                if (11..=16).contains(&shift) {
                    /* A UTF-16 unit is complete. */
                    let unit = acc >> 16;
                    if is_low(unit) {
                        return Err(Stop::Invalid(pos));
                    }
                    if !is_high(unit) {
                        out.push(char::from_u32(unit).expect("not a surrogate"));
                        self.acc = acc << 16;
                        self.shift = shift + 16;
                        pos += 1;
                        continue;
                    }
                } else if (5..=10).contains(&shift) && !is_low(acc & 0xffff) {
                    /* What follows a high surrogate is not a low one. */
                    return Err(Stop::Invalid(pos));
                }
                self.acc = acc;
                self.shift = shift;
            } else {
                /* A surrogate pair is complete. */
                let high = self.acc >> 16;
                let low = (self.acc & 0xffff) | (i >> (6 - self.shift));
                self.acc = (i << self.shift) << 26;
                self.shift += 26;
                let c = 0x10000 + ((high - 0xd800) << 10) + (low - 0xdc00);
                out.push(char::from_u32(c).expect("a supplementary character"));
            }
            pos += 1;
        }
        Ok(())
    }
}

/// Converts `input` to UTF-8 as mewl does with iconv. With
/// `keep_invalid`, an invalid byte is kept as it is and the conversion
/// goes on, and the rest of an incomplete sequence at the end is kept;
/// otherwise `None` is returned for them.
pub fn convert(variant: Variant, input: &[u8], keep_invalid: bool) -> Option<Vec<u8>> {
    let mut decoder = Decoder::new(variant);
    let mut out = Vec::with_capacity(input.len() * 2);
    let mut text = String::new();
    let mut pos = 0;
    loop {
        text.clear();
        let stop = decoder.decode(input, pos, &mut text);
        out.extend_from_slice(text.as_bytes());
        match stop {
            Ok(()) => return Some(out),
            Err(_) if !keep_invalid => return None,
            Err(Stop::Invalid(p)) => {
                out.push(input[p]);
                pos = p + 1;
            }
            Err(Stop::Incomplete(p)) => {
                out.extend_from_slice(&input[p..]);
                return Some(out);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strict(s: &str) -> Option<Vec<u8>> {
        convert(Variant::Utf7, s.as_bytes(), false)
    }

    fn kept(s: &str) -> Vec<u8> {
        convert(Variant::Utf7, s.as_bytes(), true).unwrap()
    }

    #[test]
    fn valid() {
        assert_eq!(strict("+ZeVnLIqe-").unwrap(), "日本語".as_bytes());
        assert_eq!(strict("+ZeVnLIqe").unwrap(), "日本語".as_bytes());
        assert_eq!(strict("x+AGEAYgBj-y").unwrap(), b"xabcy");
        assert_eq!(strict("caf+AOk-").unwrap(), "café".as_bytes());
        assert_eq!(strict("a+-b").unwrap(), b"a+b");
        assert_eq!(strict("+!-").unwrap(), b"!-");
        assert_eq!(strict("x+AGE.y").unwrap(), b"xa.y");
        assert_eq!(strict("+2D3cAA-").unwrap(), "\u{1f400}".as_bytes());
        /* Bits pending at the end are dropped. */
        assert_eq!(strict("+Ze").unwrap(), b"");
    }

    #[test]
    fn invalid_as_glibc() {
        assert_eq!(strict("+ZeV-"), None);
        assert_eq!(kept("+ZeV-"), "日-".as_bytes());
        assert_eq!(kept("a~b"), b"a~b");
        assert_eq!(kept("a\\b"), b"a\\b");
        assert_eq!(kept("+"), b"+");
        assert_eq!(kept("a+"), b"a+");
        assert_eq!(kept("+2D0-"), b"-");
        assert_eq!(kept("+2D0AYQ-"), b"AYQ-");
        assert_eq!(kept("+AGE~"), b"a~");
        assert_eq!(kept("\u{e9}+AGE-"), "\u{e9}a".as_bytes());
    }

    #[test]
    fn lone_low_surrogate() {
        assert_eq!(strict("+3AA-"), None);
        /* The rest of base64 is kept as it is. */
        assert_eq!(kept("+3AA-"), b"A-");
        assert_eq!(kept("+3AAAYQ-x"), b"AAYQ-x");
    }

    #[test]
    fn imap() {
        let imap = |s: &str, keep| convert(Variant::Imap, s.as_bytes(), keep);
        assert_eq!(imap("&ZeVnLIqe-", false).unwrap(), "日本語".as_bytes());
        assert_eq!(imap("a+b&-c", false).unwrap(), b"a+b&c");
        assert_eq!(imap("&AGE.", false), None);
        assert_eq!(Variant::for_charset(b"UTF-7-Imap"), Some(Variant::Imap));
        assert_eq!(Variant::for_charset(b"Utf7"), Some(Variant::Utf7));
        assert_eq!(Variant::for_charset(b"x-utf-7"), None);
    }
}
