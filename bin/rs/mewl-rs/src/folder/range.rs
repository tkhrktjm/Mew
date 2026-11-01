//! `+folder range`.

use crate::bytes::BytesExt;
use crate::compat::str_to_uint;
use crate::error::{Error, Result};

const INBOX: &[u8] = b"+inbox";

/// Emacs passes a space in a folder name as `\007` since a space
/// separates a folder and a range in `-s`.
const SPACE_IN_FOLDER: u8 = 0o7;

/// A folder and messages in it: `+folder range`.
#[derive(Clone)]
pub struct FolderRange {
    pub folder: Vec<u8>,
    /// The lowest message number, 0 means no limit.
    pub first: u32,
    /// The highest message number, 0 means no limit.
    pub last: u32,
    /// The number of the last messages (`last:N`), 0 means all.
    pub count: u32,
}

impl FolderRange {
    /// `folder` may contain a range after a space. Otherwise,
    /// `range` is used:
    ///
    /// `range = N | [start]-[end] | last:N`
    ///
    /// Numbers are read by `str_to_uint()` as mewl does: garbage is 0,
    /// and a negative number or one too large is an error.
    pub fn parse(folder: Option<&[u8]>, range: Option<&[u8]>) -> Result<Self> {
        Self::parse_with(folder, range, str_to_uint, str_to_uint)
    }

    /// [`FolderRange::parse`] accepting only decimal numbers that fit.
    /// `start` and `end` may be omitted.
    pub fn parse_strict(folder: Option<&[u8]>, range: Option<&[u8]>) -> Result<Self> {
        let invalid = || {
            let range = folder.and_then(|f| f.split_once_byte(b' ').1).or(range);
            Error::InvalidRange(range.unwrap_or_default().to_vec())
        };
        let num = |s: &[u8]| parse_number(s).ok_or_else(invalid);
        let optional = |s: &[u8]| if s.is_empty() { Ok(0) } else { num(s) };
        Self::parse_with(folder, range, num, optional)
    }

    /// `num` reads N; `optional` reads start or end.
    fn parse_with(
        folder: Option<&[u8]>,
        range: Option<&[u8]>,
        num: impl Fn(&[u8]) -> Result<u32>,
        optional: impl Fn(&[u8]) -> Result<u32>,
    ) -> Result<Self> {
        let (folder, range) = match folder.map(|f| f.split_once_byte(b' ')) {
            Some((f, Some(r))) => (Some(f), Some(r)),
            Some((f, None)) => (Some(f), range),
            None => (None, range),
        };

        let (first, last, count) = match range {
            None => (0, 0, 0),
            Some(r) => match r.strip_prefix(b"last:") {
                Some(n) => (0, 0, num(n)?),
                None => match r.split_once_byte(b'-') {
                    (n, None) => {
                        let n = num(n)?;
                        (n, n, 0)
                    }
                    (start, Some(end)) => (optional(start)?, optional(end)?, 0),
                },
            },
        };

        let folder = folder
            .unwrap_or(INBOX)
            .iter()
            .map(|&c| if c == SPACE_IN_FOLDER { b' ' } else { c })
            .collect();

        Ok(FolderRange { folder, first, last, count })
    }

    pub fn contains(&self, num: u32) -> bool {
        num >= self.first && (self.last == 0 || num <= self.last)
    }
}

/// A decimal number without a sign that fits.
pub fn parse_number(s: &[u8]) -> Option<u32> {
    if s.is_empty() || !s.iter().all(u8::is_ascii_digit) {
        return None;
    }
    std::str::from_utf8(s).ok()?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::FolderRange;
    use crate::error::Error;

    fn range(f: &[u8], r: Option<&[u8]>) -> (Vec<u8>, u32, u32, u32) {
        let fr = FolderRange::parse(Some(f), r).unwrap();
        (fr.folder, fr.first, fr.last, fr.count)
    }

    #[test]
    fn parse_ranges() {
        assert_eq!(range(b"+inbox", None), (b"+inbox".to_vec(), 0, 0, 0));
        assert_eq!(range(b"+inbox", Some(b"3")), (b"+inbox".to_vec(), 3, 3, 0));
        assert_eq!(range(b"+inbox 2-5", Some(b"9")), (b"+inbox".to_vec(), 2, 5, 0));
        assert_eq!(range(b"+inbox", Some(b"-5")), (b"+inbox".to_vec(), 0, 5, 0));
        assert_eq!(range(b"+inbox", Some(b"10-")), (b"+inbox".to_vec(), 10, 0, 0));
        assert_eq!(range(b"+inbox", Some(b"last:4")), (b"+inbox".to_vec(), 0, 0, 4));
        assert_eq!(range(b"+my\x07box", None), (b"+my box".to_vec(), 0, 0, 0));
        assert_eq!(FolderRange::parse(None, None).unwrap().folder, b"+inbox");
        /* as str_to_uint() reads them */
        assert_eq!(range(b"+inbox", Some(b"x-2y")), (b"+inbox".to_vec(), 0, 2, 0));
        assert_eq!(range(b"+inbox", Some(b"last:4294967295")), (b"+inbox".to_vec(), 0, 0, u32::MAX));
        let parse = |r: &[u8]| FolderRange::parse(Some(b"+inbox"), Some(r)).map(|_| ());
        assert_eq!(parse(b"last:-3"), Err(Error::NegativeNumber(b"-3".to_vec())));
        assert_eq!(parse(b"1--3"), Err(Error::NegativeNumber(b"-3".to_vec())));
        assert_eq!(parse(b"4294967296"), Err(Error::NumberOutOfRange(b"4294967296".to_vec())));
        assert_eq!(parse(b"4294967296-x"), Err(Error::NumberOutOfRange(b"4294967296".to_vec())));
    }

    #[test]
    fn parse_strict_ranges() {
        let strict = |f: &[u8], r: Option<&[u8]>| FolderRange::parse_strict(Some(f), r).map(|fr| (fr.first, fr.last, fr.count));
        assert_eq!(strict(b"+inbox", Some(b"10-")), Ok((10, 0, 0)));
        assert_eq!(strict(b"+inbox 2-5", None), Ok((2, 5, 0)));
        assert_eq!(strict(b"+inbox", Some(b"last:4")), Ok((0, 0, 4)));
        for bad in [&b"last:-3"[..], b"-3-", b"abc", b"1-x", b"last:", b"", b"99999999999"] {
            assert_eq!(strict(b"+inbox", Some(bad)), Err(Error::InvalidRange(bad.to_vec())));
        }
    }
}
