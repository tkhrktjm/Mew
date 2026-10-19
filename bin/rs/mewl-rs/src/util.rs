/*
 * util.rs -- counterpart of utils.c
 *
 * Author:  Mew developing team
 *
 * Code:
 */

use std::cell::RefCell;
use std::io::{self, BufWriter, Write};
use std::sync::OnceLock;

pub const EOF: i32 = -1;

pub const NUL: u8 = b'\0';
pub const LF: u8 = b'\n';
pub const TAB: u8 = b'\t';
pub const SP: u8 = b' ';
pub const FILESEP: u8 = b'/';

/****************************************************************
 *
 * stdout
 *
 * stdio of C buffers stdout and flushes it on fflush() and exit().
 * This is a fully buffered writer to fd 1 which does the same.
 * Errors are ignored like printf() of C.
 */

struct RawStdout;

impl Write for RawStdout {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let n = unsafe { libc::write(1, buf.as_ptr() as *const libc::c_void, buf.len()) };
        if n < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(n as usize)
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

thread_local! {
    static OUT: RefCell<BufWriter<RawStdout>> =
        RefCell::new(BufWriter::with_capacity(8192, RawStdout));
}

pub fn out(s: &[u8]) {
    OUT.with(|o| {
        let _ = o.borrow_mut().write_all(s);
    });
}

pub fn out_flush() {
    OUT.with(|o| {
        let _ = o.borrow_mut().flush();
    });
}

/* exit() of C flushes stdout. */
pub fn exit(code: i32) -> ! {
    out_flush();
    std::process::exit(code);
}

/****************************************************************
 *
 * warning
 *
 */

static WARN_PROG: OnceLock<Vec<u8>> = OnceLock::new();

pub fn set_warn_prog(prog: &[u8]) {
    let _ = WARN_PROG.set(prog.to_vec());
}

pub fn warning(msg: &[u8]) {
    let mut e = Vec::new();
    if let Some(p) = WARN_PROG.get() {
        e.extend_from_slice(p);
        e.extend_from_slice(b": ");
    }
    e.extend_from_slice(msg);
    e.push(LF);
    let _ = io::stderr().write_all(&e);
}

pub fn warn_exit(msg: &[u8]) -> ! {
    warning(msg);
    exit(libc::EXIT_FAILURE);
}

/* warn_exit() with a byte string argument: prefix + arg + suffix */
pub fn warn_exit3(prefix: &str, arg: &[u8], suffix: &str) -> ! {
    let mut m = prefix.as_bytes().to_vec();
    m.extend_from_slice(arg);
    m.extend_from_slice(suffix.as_bytes());
    warn_exit(&m)
}

/****************************************************************
 *
 * C library equivalents
 *
 */

pub fn getprognm(filename: &[u8]) -> Vec<u8> {
    let p = match filename.iter().rposition(|&c| c == FILESEP) {
        Some(i) => &filename[i + 1..],
        None => filename,
    };
    p.to_ascii_lowercase()
}

fn isspace(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r')
}

/* atoi() of glibc, that is, (int)strtol(s, NULL, 10). */
pub fn atoi(s: &[u8]) -> i32 {
    let mut i = 0;
    while i < s.len() && isspace(s[i]) {
        i += 1;
    }
    let mut neg = false;
    if i < s.len() && (s[i] == b'+' || s[i] == b'-') {
        neg = s[i] == b'-';
        i += 1;
    }
    let mut v: i128 = 0;
    while i < s.len() && s[i].is_ascii_digit() {
        if v <= i64::MAX as i128 + 1 {
            v = v * 10 + (s[i] - b'0') as i128;
        }
        i += 1;
    }
    if neg {
        v = -v;
    }
    let l = v.clamp(i64::MIN as i128, i64::MAX as i128) as i64;
    l as i32
}

/* strncasecmp(s, &buf[pos], strlen(s)) == 0 in the C locale. */
pub fn strncasecmp_at(buf: &[u8], pos: usize, s: &[u8]) -> bool {
    for (k, &c) in s.iter().enumerate() {
        match buf.get(pos + k) {
            Some(&b) if b.eq_ignore_ascii_case(&c) => {}
            _ => return false,
        }
    }
    true
}

/****************************************************************
 *
 * Getopt
 *
 */

pub struct Getopt {
    pub optind: usize,
    pub optarg: Vec<u8>,
}

impl Getopt {
    pub fn new() -> Self {
        Getopt {
            optind: 1,
            optarg: Vec::new(),
        }
    }

    pub fn getopt(&mut self, argv: &[Vec<u8>], fmt: &[u8]) -> Option<u8> {
        self.optarg = Vec::new();
        if self.optind >= argv.len() {
            return None;
        }
        let a = &argv[self.optind];
        if a.first() != Some(&b'-') {
            return None;
        }
        let p = &a[1..];
        let c = *p.first()?;
        if p.len() > 1 {
            warn_exit3("unknown long option '-", p, "'.");
        }
        let q = match fmt.iter().position(|&f| f == c) {
            Some(q) => q,
            None => warn_exit3("unknown option '-", &[c], "'."),
        };
        if fmt.get(q + 1) == Some(&b':') {
            self.optind += 1;
            if self.optind >= argv.len() {
                warn_exit3("no parameter for '-", &[c], "'.");
            }
            self.optarg = argv[self.optind].clone();
        }
        self.optind += 1;
        Some(c)
    }
}

/*
 * Copyright (C) 2000-2005 Mew developing team.
 * All rights reserved.
 *
 * Redistribution and use in source and binary forms, with or without
 * modification, are permitted provided that the following conditions
 * are met:
 *
 * 1. Redistributions of source code must retain the above copyright
 *    notice, this list of conditions and the following disclaimer.
 * 2. Redistributions in binary form must reproduce the above copyright
 *    notice, this list of conditions and the following disclaimer in the
 *    documentation and/or other materials provided with the distribution.
 * 3. Neither the name of the team nor the names of its contributors
 *    may be used to endorse or promote products derived from this software
 *    without specific prior written permission.
 *
 * THIS SOFTWARE IS PROVIDED BY THE TEAM AND CONTRIBUTORS ``AS IS'' AND
 * ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
 * IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR
 * PURPOSE ARE DISCLAIMED.  IN NO EVENT SHALL THE TEAM OR CONTRIBUTORS BE
 * LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR
 * CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF
 * SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR
 * BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
 * WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE
 * OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN
 * IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
 */

/*
 * util.rs ends here
 */
