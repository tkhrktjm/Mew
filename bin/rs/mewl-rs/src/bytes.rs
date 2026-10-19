//! Byte strings, which file names and headers are.

/// How a value is compared with a text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Case {
    Sensitive,
    /// Ignoring the case of ASCII letters, as `strncasecmp()` in the
    /// C locale.
    Ignore,
    /// [`Case::Ignore`] as mewl on glibc: a value beginning with a
    /// byte of 0x80 or more never matches. mewl compares the first
    /// character with `tolower(c)` and `toupper(c)`, which map a
    /// negative `char` to 128..255 on glibc.
    IgnoreAsGlibc,
}

pub trait BytesExt {
    /// Without leading spaces and tabs.
    fn trim_start_blank(&self) -> &[u8];

    /// Split at the first `sep`, which is excluded.
    fn split_once_byte(&self, sep: u8) -> (&[u8], Option<&[u8]>);

    /// Whether `needle` is in `self` (`strstr()`). An empty `needle`
    /// is always found.
    fn contains_bytes(&self, needle: &[u8], case: Case) -> bool;
}

impl BytesExt for [u8] {
    fn trim_start_blank(&self) -> &[u8] {
        let n = self.iter().take_while(|&&c| c == b' ' || c == b'\t').count();
        &self[n..]
    }

    fn split_once_byte(&self, sep: u8) -> (&[u8], Option<&[u8]>) {
        match self.iter().position(|&c| c == sep) {
            Some(i) => (&self[..i], Some(&self[i + 1..])),
            None => (self, None),
        }
    }

    fn contains_bytes(&self, needle: &[u8], case: Case) -> bool {
        let Some(&first) = needle.first() else {
            return true;
        };
        let mut windows = self.windows(needle.len());
        match case {
            Case::Sensitive => windows.any(|w| w == needle),
            Case::IgnoreAsGlibc if first >= 0x80 => false,
            Case::Ignore | Case::IgnoreAsGlibc => windows.any(|w| w.eq_ignore_ascii_case(needle)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contains() {
        assert!(b"Hello World".contains_bytes(b"o W", Case::Sensitive));
        assert!(!b"Hello World".contains_bytes(b"o w", Case::Sensitive));
        assert!(b"Hello World".contains_bytes(b"o w", Case::Ignore));
        assert!(b"abc".contains_bytes(b"", Case::Sensitive));
        assert!("日本語".as_bytes().contains_bytes("本".as_bytes(), Case::Ignore));
        assert!(!"日本語".as_bytes().contains_bytes("本".as_bytes(), Case::IgnoreAsGlibc));
        assert!("x日本語".as_bytes().contains_bytes("x日".as_bytes(), Case::IgnoreAsGlibc));
    }

    #[test]
    fn split_and_trim() {
        assert_eq!(b"a b c".split_once_byte(b' '), (&b"a"[..], Some(&b"b c"[..])));
        assert_eq!(b"abc".split_once_byte(b' '), (&b"abc"[..], None));
        assert_eq!(b" \t a ".trim_start_blank(), b"a ");
    }
}
