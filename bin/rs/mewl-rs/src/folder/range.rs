//! `+folder range`.

use crate::bytes::BytesExt;
use crate::compat::atoi;
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
    /// Numbers are read by `atoi()` as mewl does: garbage is 0 and a
    /// negative number wraps around like unsigned int, so `last:-3`
    /// means all.
    pub fn parse(folder: Option<&[u8]>, range: Option<&[u8]>) -> Self {
        let num = |s: &[u8]| Some(atoi(s) as u32);
        Self::parse_with(folder, range, num, num).expect("atoi() never fails")
    }

    /// [`FolderRange::parse`] accepting only decimal numbers that fit.
    /// `start` and `end` may be omitted.
    pub fn parse_strict(folder: Option<&[u8]>, range: Option<&[u8]>) -> Result<Self> {
        let num = |s: &[u8]| parse_number(s);
        let optional = |s: &[u8]| if s.is_empty() { Some(0) } else { parse_number(s) };
        Self::parse_with(folder, range, num, optional).ok_or_else(|| {
            let range = folder.and_then(|f| f.split_once_byte(b' ').1).or(range);
            Error::InvalidRange(range.unwrap_or_default().to_vec())
        })
    }

    /// `num` reads N; `optional` reads start or end.
    fn parse_with(
        folder: Option<&[u8]>,
        range: Option<&[u8]>,
        num: impl Fn(&[u8]) -> Option<u32>,
        optional: impl Fn(&[u8]) -> Option<u32>,
    ) -> Option<Self> {
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
                    (n, None) => (num(n)?, num(n)?, 0),
                    (start, Some(end)) => (optional(start)?, optional(end)?, 0),
                },
            },
        };

        let folder = folder
            .unwrap_or(INBOX)
            .iter()
            .map(|&c| if c == SPACE_IN_FOLDER { b' ' } else { c })
            .collect();

        Some(FolderRange { folder, first, last, count })
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
        let fr = FolderRange::parse(Some(f), r);
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
        assert_eq!(FolderRange::parse(None, None).folder, b"+inbox");
        assert_eq!(range(b"+inbox", Some(b"last:-3")), (b"+inbox".to_vec(), 0, 0, u32::MAX - 2));
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
