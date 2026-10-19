//! Reading a message and finding fields in its header.
//!
//! Only the first [`MAX_HEADER`] bytes of a message are read into a
//! buffer which is reused for all messages, as mewl does. The scanning
//! below keeps two side effects of mewl since they show up in the
//! output:
//!
//! - Reaching the end of the data overwrites its last byte with LF.
//! - Comparing a field name for scan may look beyond the data into what
//!   the previous message left in the buffer.
//!
//! So positions are indices into the buffer, as pointers in mewl, and
//! the order of comparing and skipping a field is kept.

mod fields;

use std::io::{ErrorKind, Read};
use std::ops::Range;

pub use fields::{BodyLine, FieldSet, Summary};

use crate::bytes::{BytesExt, Case};
use crate::flavor::Flavor;

/// 64k, now even 16k is not enough.
pub const MAX_HEADER: usize = 65536;

const LF: u8 = b'\n';

/// The special key of a pick pattern to search the whole header.
const WHOLE_HEADER: &[u8] = b"head";

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
    /// How pick decodes values (see [`Header::search`]).
    flavor: Flavor,
}

impl Header {
    pub fn new(field_len: i32, flavor: Flavor) -> Self {
        Header {
            buf: vec![0; MAX_HEADER].into_boxed_slice(),
            len: 0,
            field_len,
            flavor,
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
        /* So that the last byte is not overwritten (see next_line()),
        unless the buffer is full. */
        if self.flavor.fixes_bugs() && len > 0 && self.buf[len - 1] != LF && len < self.buf.len() {
            self.buf[len] = LF;
            len += 1;
        }
        self.len = len;
    }

    /// The bytes in `range`; empty if it is reversed.
    pub fn bytes(&self, range: Range<usize>) -> &[u8] {
        self.buf.get(range).unwrap_or_default()
    }

    /// What may be read. mewl reads beyond the data into what the
    /// previous message left in the buffer.
    fn readable(&self) -> &[u8] {
        if self.flavor.fixes_bugs() { &self.buf[..self.len] } else { &self.buf }
    }

    /// Whether the header is empty, the message beginning with an
    /// empty line. mewl takes the body for the header then.
    fn has_empty_header(&self) -> bool {
        self.flavor.fixes_bugs() && self.len > 0 && self.buf[0] == LF
    }

    /// `strncasecmp(name, &buf[pos], strlen(name)) == 0`.
    fn has_name_at(&self, pos: usize, name: &[u8]) -> bool {
        self.readable()
            .get(pos..pos + name.len())
            .is_some_and(|s| s.eq_ignore_ascii_case(name))
    }

    /// Whether the field at `p` is named `key` (`field_is()` of mewl).
    /// The name has to end at `:`, so that `to` does not match
    /// `Token:`. `key` may spell the `:` out itself.
    fn is_field(&self, p: usize, key: &[u8]) -> bool {
        let len = key.len();
        if len == 0 || p >= self.len || len >= self.len - p {
            return false;
        }
        self.buf[p..p + len].eq_ignore_ascii_case(key) && (key.ends_with(b":") || self.buf[p + len] == b':')
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
        if self.has_empty_header() {
            return Some(0);
        }
        let data = &self.buf[..self.len];
        match data.windows(2).position(|w| w == b"\n\n") {
            Some(i) => Some(i + 1),
            None => self.len.checked_sub(1),
        }
    }

    /// The C string beginning at `beg` when NUL is put at `nul` for a
    /// while. Without `nul` after `beg`, it may extend beyond the data
    /// (see [`Header::readable`]).
    fn c_str(&self, beg: usize, nul: Option<usize>) -> &[u8] {
        let readable = self.readable();
        let end = match nul {
            Some(nul) if nul >= beg => nul.min(readable.len()),
            _ => readable.len(),
        };
        let s = readable.get(beg..end).unwrap_or_default();
        s.split(|&c| c == 0).next().unwrap_or_default()
    }

    /// The end of a field body excluding its last LF.
    fn body_end(&self, end: &FieldEnd) -> Option<usize> {
        match (end.truncated, end.next) {
            (Some(t), _) => Some(t - 1),
            (None, Some(n)) => Some(n - 1),
            (None, None) => self.len.checked_sub(1),
        }
    }

    /// For scan: finds `fields` in the header and the first `body_len`
    /// lines of the body.
    pub fn summarize(&mut self, fields: &FieldSet, body_len: i32) -> Summary {
        let mut found: Vec<Option<Range<usize>>> = vec![None; fields.names().len()];
        let mut body_beg = self.has_empty_header().then_some(1);
        let mut p = 0;

        while body_beg.is_none() {
            let matched = fields.names().iter().position(|n| self.has_name_at(p, n));
            let end = self.next_field(p, false);
            if let Some(i) = matched {
                found[i] = Some(if fields.is_references(i) {
                    /* The bottom part of References: is important.
                    Its last line only, without the name since it
                    is printed separately. */
                    let beg = if end.last_line == p {
                        p + fields.names()[i].len()
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
                        /* mewl does not escape the last line. */
                        let dot_stuffed = self.flavor.fixes_bugs() && self.buf[q..self.len].starts_with(b".");
                        body.push(BodyLine { range: q..self.len, dot_stuffed });
                        break;
                    }
                }
            }
        }

        Summary { fields: found, body }
    }

    /// For sort: the value of the first field named `key`, as mewl
    /// prints it. If `key` lacks `:`, the value begins with it.
    pub fn field_value(&mut self, key: &[u8]) -> Option<Range<usize>> {
        let mut p = 0;
        loop {
            let matched = self.is_field(p, key);
            let end = self.next_field(p, true);
            if matched {
                let stop = match (end.truncated, end.next) {
                    (Some(t), _) => t - 1,
                    (None, Some(n)) => n - 1,
                    /* Including the last byte, which is LF now. */
                    (None, None) => self.len,
                };
                /* mewl begins the value with ":" if the key lacks it. */
                let colon = usize::from(self.flavor.fixes_bugs() && !key.ends_with(b":"));
                return Some(p + key.len() + colon..stop);
            }
            p = end.next?;
        }
    }

    /// For pick: whether a field named `key` contains `value`. The key
    /// `head` means the whole header. How a value is decoded and
    /// compared depends on the [`Flavor`].
    pub fn search(&mut self, key: &[u8], value: &[u8], case_sensitive: bool) -> bool {
        let case = if case_sensitive { Case::Sensitive } else { self.flavor.ignore_case() };

        if key == WHOLE_HEADER {
            let header = self.c_str(0, self.end_of_header());
            return self.flavor.whole_header(header).contains_bytes(value, case);
        }

        let mut p = 0;
        loop {
            let matched = self.is_field(p, key);
            let end = self.next_field(p, true);
            if matched {
                let body = self.c_str(p + key.len(), self.body_end(&end));
                if self.flavor.field_body(body).contains_bytes(value, case) {
                    return true;
                }
            }
            p = match end.next {
                Some(next) => next,
                None => return false,
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(mut data: &[u8], field_len: i32, flavor: Flavor) -> Header {
        let mut h = Header::new(field_len, flavor);
        h.load(&mut data);
        h
    }

    #[test]
    fn summarize_fields_and_body() {
        let mut h = header(b"Subject: a\n b\n c\nReferences: <1>\n <2>\nX: y\n\n.dot\nline\nlast", 2, Flavor::Compat);
        let fields = FieldSet::parse(b"Subject:,References:");
        let s = h.summarize(&fields, 5);
        assert_eq!(h.bytes(s.fields[0].clone().unwrap()), b"Subject: a\n b\n");
        assert_eq!(h.bytes(s.fields[1].clone().unwrap()), b" <2>\n");
        let body: Vec<_> = s.body.iter().map(|l| (h.bytes(l.range.clone()).to_vec(), l.dot_stuffed)).collect();
        /* The last byte is overwritten with LF as mewl does. */
        assert_eq!(body, [(b".dot\n".to_vec(), true), (b"line\n".to_vec(), false), (b"las\n".to_vec(), false)]);
    }

    #[test]
    fn last_line_kept() {
        let data = b"Subject: abc\n\nline\n.";
        let fields = FieldSet::parse(b"Subject:");
        let mut compat = header(data, 5, Flavor::Compat);
        let s = compat.summarize(&fields, 5);
        let last = &s.body[1];
        assert_eq!((compat.bytes(last.range.clone()), last.dot_stuffed), (&b"\n"[..], false));

        let mut utf8 = header(data, 5, Flavor::Utf8);
        let s = utf8.summarize(&fields, 5);
        let last = &s.body[1];
        assert_eq!((utf8.bytes(last.range.clone()), last.dot_stuffed), (&b".\n"[..], true));

        let mut utf8 = header(b"Subject: abc", 5, Flavor::Utf8);
        assert!(utf8.search(b"subject", b"abc", true));
        let v = utf8.field_value(b"subject:").unwrap();
        assert_eq!(utf8.bytes(v), b" abc\n");
    }

    #[test]
    fn search_and_value() {
        let mut h = header(b"Subject: Hello World\nFrom: foo\nFrom-Address: bar\n\nbody hello\n", 5, Flavor::Compat);
        assert!(h.search(b"subject", b"hello", false));
        assert!(!h.search(b"subject", b"hello", true));
        assert!(!h.search(b"from", b"bar", false));
        assert!(!h.search(b"subj", b"hello", false));
        assert!(h.search(b"head", b"FOO", false));
        assert!(!h.search(b"head", b"body", false));
        let v = h.field_value(b"from:").unwrap();
        assert_eq!(h.bytes(v), b" foo");
        assert!(h.field_value(b"to:").is_none());
    }

    #[test]
    fn search_decoded() {
        let data = "Subject: =?UTF-8?B?5pel5pys6Kqe?=\nX: Re:=?UTF-8?B?5pel?=\n\n".as_bytes();
        let mut mewl = header(data, 5, Flavor::Compat);
        let mut utf8 = header(data, 5, Flavor::Utf8);
        assert!(mewl.search(b"subject", "日本".as_bytes(), true));
        assert_eq!(mewl.search(b"subject", "日本".as_bytes(), false), !crate::compat::GLIBC);
        assert!(utf8.search(b"subject", "日本".as_bytes(), false));
        assert!(!mewl.search(b"x", "日".as_bytes(), true));
        assert!(utf8.search(b"x", "Re:日".as_bytes(), true));
        assert!(!mewl.search(b"head", "日本".as_bytes(), true));
        assert!(utf8.search(b"head", "日本".as_bytes(), true));
    }
}
