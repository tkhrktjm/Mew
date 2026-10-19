//! Behavior of the C library that mewl relies on.

/// `atoi()` of glibc, that is, `(int)strtol(s, NULL, 10)`.
///
/// Garbage gives 0, an overflow saturates to `long` and then is
/// truncated to `int`.
pub fn atoi(s: &[u8]) -> i32 {
    let blanks = s.iter().take_while(|&&c| isspace(c)).count();
    let s = &s[blanks..];
    let (negative, digits) = match s.split_first() {
        Some((b'-', rest)) => (true, rest),
        Some((b'+', rest)) => (false, rest),
        _ => (false, s),
    };
    let value = digits
        .iter()
        .take_while(|c| c.is_ascii_digit())
        .fold(0i128, |acc, &c| (acc * 10 + i128::from(c - b'0')).min(1 << 64));
    let value = if negative { -value } else { value };
    let long = value.clamp(i64::MIN.into(), i64::MAX.into()) as i64;
    long as i32
}

/// `isspace()` in the C locale. Unlike `u8::is_ascii_whitespace()`,
/// this includes the vertical tab.
fn isspace(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r')
}

/// Whether `char` is signed in C on this platform.
pub const CHAR_IS_SIGNED: bool = std::ffi::c_char::MIN != 0;

/// Whether mewl would be built with glibc and signed `char`, where
/// `tolower()` of a byte >= 0x80 is not the byte itself (see
/// [`crate::bytes::Case::IgnoreAsGlibc`]).
pub const GLIBC: bool = cfg!(all(target_os = "linux", target_env = "gnu")) && CHAR_IS_SIGNED;

#[cfg(test)]
mod tests {
    use super::atoi;

    #[test]
    fn atoi_like_glibc() {
        assert_eq!(atoi(b"123"), 123);
        assert_eq!(atoi(b"  -42abc"), -42);
        assert_eq!(atoi(b"\x0b5"), 5);
        assert_eq!(atoi(b"+7"), 7);
        assert_eq!(atoi(b"abc"), 0);
        assert_eq!(atoi(b""), 0);
        assert_eq!(atoi(b"99999999999"), 1215752191);
        assert_eq!(atoi(b"99999999999999999999"), -1);
        assert_eq!(atoi(b"-99999999999999999999"), 0);
    }
}
