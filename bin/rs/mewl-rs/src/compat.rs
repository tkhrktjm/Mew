//! Behavior of the C library that mewl relies on.

use crate::error::{Error, Result};

/// `str_to_uint()` of mewl, by `strtoul()` of glibc.
///
/// Garbage gives 0 as `atoi()` did, and what follows the digits is
/// ignored, but a negative number or one too large for `unsigned int`
/// is an error.
pub fn str_to_uint(s: &[u8]) -> Result<u32> {
    let blanks = s.iter().take_while(|&&c| isspace(c)).count();
    let rest = &s[blanks..];
    let digits = match rest.split_first() {
        Some((b'-', _)) => return Err(Error::NegativeNumber(s.to_vec())),
        Some((b'+', digits)) => digits,
        _ => rest,
    };
    digits
        .iter()
        .take_while(|c| c.is_ascii_digit())
        .try_fold(0u32, |n, &c| n.checked_mul(10)?.checked_add(u32::from(c - b'0')))
        .ok_or_else(|| Error::NumberOutOfRange(s.to_vec()))
}

/// `isspace()` in the C locale. Unlike `u8::is_ascii_whitespace()`,
/// this includes the vertical tab.
fn isspace(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r')
}

/// `BUFSIZ` of glibc.
pub const BUFSIZ: usize = 8192;

#[cfg(test)]
mod tests {
    use super::str_to_uint;
    use crate::error::Error;

    #[test]
    fn str_to_uint_like_mewl() {
        assert_eq!(str_to_uint(b"123"), Ok(123));
        assert_eq!(str_to_uint(b"  42abc"), Ok(42));
        assert_eq!(str_to_uint(b"\x0b5"), Ok(5));
        assert_eq!(str_to_uint(b"+7"), Ok(7));
        assert_eq!(str_to_uint(b"abc"), Ok(0));
        assert_eq!(str_to_uint(b""), Ok(0));
        assert_eq!(str_to_uint(b"+-3"), Ok(0));
        assert_eq!(str_to_uint(b"4294967295"), Ok(u32::MAX));
        assert_eq!(str_to_uint(b"4294967296"), Err(Error::NumberOutOfRange(b"4294967296".to_vec())));
        assert_eq!(str_to_uint(b"99999999999999999999x"), Err(Error::NumberOutOfRange(b"99999999999999999999x".to_vec())));
        assert_eq!(str_to_uint(b" -3"), Err(Error::NegativeNumber(b" -3".to_vec())));
        assert_eq!(str_to_uint(b"-0"), Err(Error::NegativeNumber(b"-0".to_vec())));
    }
}
