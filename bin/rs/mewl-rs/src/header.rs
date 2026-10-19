//! Reading a message and finding fields in its header.
//!
//! Only the first [`MAX_HEADER`] bytes of a message are read into a
//! buffer which is reused for all messages, as mewl does. The scanning
//! below keeps two side effects of mewl since they show up in the
//! output:
//!
//! - Reaching the end of the data overwrites its last byte with LF.
//! - Comparing a field name may look beyond the data into what the
//!   previous message left in the buffer.

use std::io::{ErrorKind, Read};
use std::ops::Range;

use crate::compat::CHAR_IS_SIGNED;

/// 64k, now even 16k is not enough.
pub const MAX_HEADER: usize = 65536;

const LF: u8 = b'\n';

/// The special key of a pick pattern to search the whole header.
const WHOLE_HEADER: &[u8] = b"head";

/****************************************************************
 *
 * Fields to be extracted
 *
 */

pub struct FieldSet {
    names: Vec<Vec<u8>>,
    /// For `References:`, only the last line is extracted.
    references: Option<usize>,
}

impl FieldSet {
    pub const DEFAULT: &[u8] = b"Subject:,Date:,From:,To:,Cc:,Content-Type:,Content-Transfer-Encoding:,X-Mew-UIDL:,Message-ID:,In-Reply-To:,References:,X-Mew-Ref:";

    pub const REFERENCES: &[u8] = b"References:";

    /// `names` is comma-separated.
    pub fn parse(names: &[u8]) -> Self {
        let names: Vec<Vec<u8>> = names.split(|&c| c == b',').map(<[u8]>::to_vec).collect();
        let references = names.iter().rposition(|n| n == Self::REFERENCES);
        FieldSet { names, references }
    }

    pub fn is_references(&self, i: usize) -> bool {
        self.references == Some(i)
    }
}

/// Fields and the beginning of the body found by [`Header::summarize`].
pub struct Summary {
    /// The bytes to be printed for each field in [`FieldSet`].
    pub fields: Vec<Option<Range<usize>>>,
    pub body: Vec<BodyLine>,
}

pub struct BodyLine {
    pub range: Range<usize>,
    /// A line beginning with `.` is escaped as `..`.
    pub dot_stuffed: bool,
}

/****************************************************************
 *
 * Header buffer
 *
 */

/// Where a field ends.
struct FieldEnd {
    /// The beginning of the next field, `None` at the end.
    next: Option<usize>,
    /// The end of the first `field_len` lines if the field is longer.
    truncated: Option<usize>,
    /// The beginning of the last line of the field.
    last_line: usize,
}

pub struct Header {
    buf: Box<[u8]>,
    len: usize,
    /// The max number of lines of a field. 0 means no limit.
    field_len: i32,
}

impl Header {
    pub fn new(field_len: i32) -> Self {
        Header {
            buf: vec![0; MAX_HEADER].into_boxed_slice(),
            len: 0,
            field_len,
        }
    }

    /// Reads the beginning of a message like `fread()`.
    pub fn load(&mut self, message: &mut impl Read) {
        let mut len = 0;
        while len < self.buf.len() {
            match message.read(&mut self.buf[len..]) {
                Ok(0) => break,
                Ok(n) => len += n,
                Err(e) if e.kind() == ErrorKind::Interrupted => {}
                Err(_) => break,
            }
        }
        self.len = len;
    }

    /// The bytes in `range`; empty if it is reversed.
    pub fn bytes(&self, range: Range<usize>) -> &[u8] {
        self.buf.get(range).unwrap_or_default()
    }

    /// `strncasecmp(name, &buf[pos], strlen(name)) == 0`.
    /// This may look beyond the data.
    fn has_name_at(&self, pos: usize, name: &[u8]) -> bool {
        self.buf
            .get(pos..pos + name.len())
            .is_some_and(|s| s.eq_ignore_ascii_case(name))
    }

    /// The beginning of the line after `p`. `None` if the data or,
    /// with `header_only`, the header ends.
    fn next_line(&mut self, p: usize, header_only: bool) -> Option<usize> {
        if p == self.len || (header_only && self.buf[p] == LF) {
            return None;
        }
        let next = match self.buf[p..self.len].iter().position(|&c| c == LF) {
            Some(i) => p + i + 1,
            None => self.len + 1,
        };
        if next < self.len {
            Some(next)
        } else {
            self.buf[self.len - 1] = LF;
            None
        }
    }

    /// Skips the field beginning at `p` with its continuation lines.
    fn next_field(&mut self, p: usize, header_only: bool) -> FieldEnd {
        let mut lines: i32 = 0;
        let mut truncated = None;
        let mut line = p;
        loop {
            let Some(next) = self.next_line(line, header_only) else {
                return FieldEnd { next: None, truncated, last_line: line };
            };
            lines = lines.wrapping_add(1);
            if self.field_len != 0 && lines == self.field_len {
                truncated = Some(next);
            }
            if !matches!(self.buf[next], b' ' | b'\t') {
                return FieldEnd { next: Some(next), truncated, last_line: line };
            }
            line = next;
        }
    }

    /// The position of the empty line ending the header, or of the
    /// last byte. `None` if there is no data.
    fn end_of_header(&self) -> Option<usize> {
        let data = &self.buf[..self.len];
        match data.windows(2).position(|w| w == b"\n\n") {
            Some(i) => Some(i + 1),
            None => self.len.checked_sub(1),
        }
    }

    /// The C string beginning at `beg` when NUL is put at `nul` for a
    /// while. Without `nul` before it, it may extend beyond the data.
    fn c_str(&self, beg: usize, nul: Option<usize>) -> &[u8] {
        let end = match nul {
            Some(nul) if nul >= beg => nul,
            _ => self.buf.len(),
        };
        let s = self.buf.get(beg..end).unwrap_or_default();
        s.split(|&c| c == 0).next().unwrap_or_default()
    }

    /****************************************************************
     *
     * For scan
     *
     */

    /// Finds `fields` in the header and the first `body_len` lines of
    /// the body.
    pub fn summarize(&mut self, fields: &FieldSet, body_len: i32) -> Summary {
        let mut found: Vec<Option<Range<usize>>> = vec![None; fields.names.len()];
        let mut body_beg = None;
        let mut p = 0;

        loop {
            let matched = fields.names.iter().position(|n| self.has_name_at(p, n));
            let end = self.next_field(p, false);
            if let Some(i) = matched {
                found[i] = Some(if fields.is_references(i) {
                    /* The bottom part of References: is important.
                    Its last line only, without the name since it
                    is printed separately. */
                    let beg = if end.last_line == p {
                        p + fields.names[i].len()
                    } else {
                        end.last_line
                    };
                    beg..end.next.unwrap_or(self.len)
                } else {
                    p..end.truncated.or(end.next).unwrap_or(self.len)
                });
            }

            let Some(next) = end.next else { break };
            p = next;
            if self.buf[p] == LF {
                body_beg = Some(p + 1);
                break;
            }
        }

        let mut body = Vec::new();
        if let Some(mut p) = body_beg {
            /* A negative length means no limit like unsigned in C. */
            for _ in 0..body_len as u32 {
                let q = p;
                match self.next_line(q, false) {
                    Some(next) => {
                        body.push(BodyLine { range: q..next, dot_stuffed: self.buf[q] == b'.' });
                        p = next;
                    }
                    None => {
                        /* The last line is not escaped. */
                        body.push(BodyLine { range: q..self.len, dot_stuffed: false });
                        break;
                    }
                }
            }
        }

        Summary { fields: found, body }
    }

    /****************************************************************
     *
     * For sort
     *
     */

    /// The value of the first field named `key` (`key` may lack `:`).
    pub fn field_value(&mut self, key: &[u8]) -> Option<Range<usize>> {
        let mut p = 0;
        loop {
            let matched = self.has_name_at(p, key);
            let end = self.next_field(p, true);
            if matched {
                let stop = match (end.truncated, end.next) {
                    (Some(t), _) => t - 1,
                    (None, Some(n)) => n - 1,
                    (None, None) => self.len,
                };
                return Some(p + key.len()..stop);
            }
            p = end.next?;
        }
    }

    /****************************************************************
     *
     * For pick
     *
     */

    /// Whether a field named `key` contains `value`. The key `head`
    /// means the whole header.
    pub fn search(&mut self, key: &[u8], value: &[u8], case_sensitive: bool) -> bool {
        if key == WHOLE_HEADER {
            let header = self.c_str(0, self.end_of_header());
            return contains(header, value, case_sensitive);
        }

        let mut p = 0;
        loop {
            let matched = self.has_name_at(p, key);
            let end = self.next_field(p, true);
            if matched {
                /* Excluding the last LF. */
                let stop = match (end.truncated, end.next) {
                    (Some(t), _) => Some(t - 1),
                    (None, Some(n)) => Some(n - 1),
                    (None, None) => self.len.checked_sub(1),
                };
                if contains(self.c_str(p + key.len(), stop), value, case_sensitive) {
                    return true;
                }
            }
            match end.next {
                Some(next) => p = next,
                None => return false,
            }
        }
    }
}

/*
 * mewl compares the first character of the value with tolower(c) and
 * toupper(c). When char is signed, glibc maps -128..-1 to 128..255,
 * so a value beginning with a byte >= 0x80 never matches ignoring
 * case.
 */
const HIGH_FIRST_BYTE_NEVER_MATCHES: bool =
    cfg!(all(target_os = "linux", target_env = "gnu")) && CHAR_IS_SIGNED;

/// `strstr()` or `strcasestr()` in the C locale.
fn contains(haystack: &[u8], needle: &[u8], case_sensitive: bool) -> bool {
    let Some(&first) = needle.first() else {
        return true;
    };
    let mut windows = haystack.windows(needle.len());
    if case_sensitive {
        windows.any(|w| w == needle)
    } else if HIGH_FIRST_BYTE_NEVER_MATCHES && first >= 0x80 {
        false
    } else {
        windows.any(|w| w.eq_ignore_ascii_case(needle))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(mut data: &[u8], field_len: i32) -> Header {
        let mut h = Header::new(field_len);
        h.load(&mut data);
        h
    }

    #[test]
    fn summarize_fields_and_body() {
        let mut h = header(b"Subject: a\n b\n c\nReferences: <1>\n <2>\nX: y\n\n.dot\nline\nlast", 2);
        let fields = FieldSet::parse(b"Subject:,References:");
        let s = h.summarize(&fields, 5);
        assert_eq!(h.bytes(s.fields[0].clone().unwrap()), b"Subject: a\n b\n");
        assert_eq!(h.bytes(s.fields[1].clone().unwrap()), b" <2>\n");
        let body: Vec<_> = s.body.iter().map(|l| (h.bytes(l.range.clone()).to_vec(), l.dot_stuffed)).collect();
        assert_eq!(body, [(b".dot\n".to_vec(), true), (b"line\n".to_vec(), false), (b"las\n".to_vec(), false)]);
    }

    #[test]
    fn search_and_value() {
        let mut h = header(b"Subject: Hello World\nFrom: foo\n\nbody hello\n", 5);
        assert!(h.search(b"subject", b"hello", false));
        assert!(!h.search(b"subject", b"hello", true));
        assert!(!h.search(b"from", b"hello", false));
        assert!(h.search(b"head", b"FOO", false));
        assert!(!h.search(b"head", b"body", false));
        let v = h.field_value(b"from:").unwrap();
        assert_eq!(h.bytes(v), b" foo");
        assert!(h.field_value(b"to:").is_none());
    }
}
