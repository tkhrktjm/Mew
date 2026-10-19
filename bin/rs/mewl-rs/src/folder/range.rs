//! `+folder range`.

use crate::bytes::BytesExt;
use crate::compat::atoi;

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
    pub fn parse(folder: Option<&[u8]>, range: Option<&[u8]>) -> Self {
        let (folder, range) = match folder.map(|f| f.split_once_byte(b' ')) {
            Some((f, Some(r))) => (Some(f), Some(r)),
            Some((f, None)) => (Some(f), range),
            None => (None, range),
        };

        /* Negative numbers wrap around like unsigned int in C. */
        let num = |s: &[u8]| atoi(s) as u32;
        let (first, last, count) = match range {
            None => (0, 0, 0),
            Some(r) => match r.strip_prefix(b"last:") {
                Some(n) => (0, 0, num(n)),
                None => match r.split_once_byte(b'-') {
                    (n, None) => (num(n), num(n), 0),
                    (start, Some(end)) => (num(start), num(end), 0),
                },
            },
        };

        let folder = folder
            .unwrap_or(INBOX)
            .iter()
            .map(|&c| if c == SPACE_IN_FOLDER { b' ' } else { c })
            .collect();

        FolderRange { folder, first, last, count }
    }

    pub fn contains(&self, num: u32) -> bool {
        num >= self.first && (self.last == 0 || num <= self.last)
    }
}

#[cfg(test)]
mod tests {
    use super::FolderRange;

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
    }
}
