/*
 * mewl-rs -- Message scanner for Mew 4 or later
 *
 * A Rust port of mewl.c. The input, the output, and the behavior
 * for options and their arguments are the same as mewl.
 *
 * Author:  Mew developing team
 *
 * Code:
 */

mod pattern;
mod util;

use std::ffi::OsStr;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Read};
use std::os::fd::FromRawFd;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::os::unix::fs::MetadataExt;
use std::process::{Command, Stdio};

use pattern::Pattern;
use util::*;

const VERSION_MESSAGE: &str = "version 6.11 20260414 Mew developing team";

/****************************************************************
 *
 * Macro, structure, and tables
 *
 */

const MAILHOME: &[u8] = b"Mail";
const CHDIR: &[u8] = b"CD:";
const INBOX: &[u8] = b"+inbox";
const STDINFILE: &[u8] = b"-";
const HEAD: &[u8] = b"head";
const FILE_SEPARATOR: u8 = 0o7;
const SUFFIX: &[u8] = b".mew";

/* 64k, now even 16k is not enough. */
const MAX_HEADER: usize = 65536;
const MAX_FIELD_LEN: i32 = 5;
const MAX_BODY_LEN: i32 = 20;
const MAX_FOLDER: usize = 16;
const MAX_ARGC: usize = 16;
const MAX_FILE_NAME_LEN: usize = libc::PATH_MAX as usize;

const DEFAULT_FIELDS: &[u8] =
    b"Subject:,Date:,From:,To:,Cc:,Content-Type:,Content-Transfer-Encoding:,X-Mew-UIDL:,Message-ID:,In-Reply-To:,References:,X-Mew-Ref:";

const REFERENCES: &[u8] = b"References:";

const S_IFREG: u32 = 0o100000;

struct FldRng {
    folder: Vec<u8>,
    first: u32,
    last: u32,
    num: u32,
}

#[derive(Clone, Copy, PartialEq)]
enum Printer {
    Scan,
    Sort,
    Pick,
}

#[derive(Clone, Copy, PartialEq)]
enum Source {
    Scan,
    Exec,
    Stdin,
}

struct Mewl {
    /* Buf_header is shared by all messages like the static buffer of C. */
    buf: Vec<u8>,
    mail_home: Vec<u8>,
    current_folder: Vec<u8>,
    folders: Vec<FldRng>,

    key_field: Vec<u8>,
    exec_cmd: Vec<u8>,
    exec_arg: Option<Vec<u8>>,
    input_file: Vec<u8>,
    suffix: Vec<u8>,

    all_fields: i32,
    field_len: i32,
    body_len: i32,
    print_num_of_msg: bool,

    no_fld_flag: bool,
    default_fld_error_flag: bool,

    /* print sub-functions */
    fields: Vec<Vec<u8>>,
    ref_idx: Option<usize>,

    /* pick */
    pattern: Option<Pattern>,
    search_lim: usize,

    /* scan functions */
    scan_folders: usize,
    scan_messages: usize,
    scan_buf: Vec<u32>,
    scan_i: usize,
    scan_j: usize,

    /* exec and stdin functions */
    input: Option<Box<dyn BufRead>>,
}

/****************************************************************
 *
 * options and usages
 *
 */

fn usage(progname: &[u8]) {
    let mut m = b"Usage: ".to_vec();
    m.extend_from_slice(progname);
    m.extend_from_slice(b" [options] [folder [range]]\n");
    err(&m);
}

const HELP_MESSAGE: &[&str] = &[
    " -a            Print all necessary fields when picking.",
    " -b  <mdir>    Mail home.",
    " -c  <bodylen> Max body length to dump.",
    " -d  <field>   A field to be extracted.",
    " -e  <command> An external command to fetch mailbox.",
    " -m  <options> The second argument for the external command.",
    " -f  <fields>  Fields to display.",
    " -h            Display this help message.",
    " -i  <file>    A file name to be read.",
    " -l  <length>  Field max length to dump. (0 means no limit)",
    " -n            Don't use fstat().",
    " -p  <pattern> Pick pattern.",
    " -s  <src>     Message source: '+folder range'.",
    " -w            Wait for stdio input to synchronize.",
    " -v            Display the version.",
    " -x            Use this suffix.",
    "",
    "<range> = N | [start]-[end] | last:N",
    "<pattern> = 'key=val' | 'key!=val' | '!<pattern>' | '(<pattern>)' ",
    "          | '<pattern>&<pattern>' ",
    "          | '<pattern>|<pattern>' ",
    "",
    "Default is mdir = ~/Mail,",
    "length = 5, folder = +inbox, range = 0, bodylen = 20",
    "field = Subject:,Date:,From:,To:,Cc:,Content-Type:,Content-Transfer-Encoding:,X-Mew-UIDL:,Message-ID:,In-Reply-To:,References:,X-Mew-Ref:",
];

fn help(progname: &[u8]) {
    let mut m = b"Help: ".to_vec();
    m.extend_from_slice(progname);
    m.extend_from_slice(b"\n\n");
    m.extend_from_slice(b" Message scanner.\n\n");
    err(&m);
    usage(progname);
    for l in HELP_MESSAGE {
        err(l.as_bytes());
        err(b"\n");
    }
}

fn version(progname: &[u8]) {
    let mut m = progname.to_vec();
    m.push(SP);
    m.extend_from_slice(VERSION_MESSAGE.as_bytes());
    m.push(LF);
    err(&m);
}

fn err(s: &[u8]) {
    use std::io::Write;
    let _ = io::stderr().write_all(s);
}

/****************************************************************
 *
 * system call wrappers
 *
 */

fn chdir(dir: &[u8]) -> bool {
    std::env::set_current_dir(OsStr::from_bytes(dir)).is_ok()
}

fn fopen(name: &[u8]) -> Option<File> {
    File::open(OsStr::from_bytes(name)).ok()
}

/* fread(buf, 1, buf.len(), fp) */
fn fread(fp: &mut File, buf: &mut [u8]) -> usize {
    let mut n = 0;
    while n < buf.len() {
        match fp.read(&mut buf[n..]) {
            Ok(0) => break,
            Ok(m) => n += m,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => break,
        }
    }
    n
}

/* fgets(buf, size, fp) */
fn fgets(fp: &mut dyn BufRead, size: usize) -> Option<Vec<u8>> {
    let mut s = Vec::new();
    while s.len() < size - 1 {
        let avail = match fp.fill_buf() {
            Ok(a) => a,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => break,
        };
        if avail.is_empty() {
            break;
        }
        let room = (size - 1 - s.len()).min(avail.len());
        let (take, eol) = match avail[..room].iter().position(|&c| c == LF) {
            Some(i) => (i + 1, true),
            None => (room, false),
        };
        s.extend_from_slice(&avail[..take]);
        fp.consume(take);
        if eol {
            break;
        }
    }
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

/* while ((c = getchar()) != LF && c != EOF) ; */
fn skip_line(fp: &mut dyn BufRead) {
    let mut v = Vec::new();
    let _ = fp.read_until(LF, &mut v);
}

fn ch_home() {
    let mut home = std::env::var_os("HOME").map(|h| h.into_vec());
    if home.as_ref().is_none_or(|h| h.is_empty()) {
        let pw = unsafe { libc::getpwuid(libc::getuid()) };
        if pw.is_null() {
            warn_exit(b"failed in getting home directory.");
        }
        let dir = unsafe { (*pw).pw_dir };
        home = if dir.is_null() {
            None
        } else {
            Some(unsafe { std::ffi::CStr::from_ptr(dir) }.to_bytes().to_vec())
        };
    }
    let home = match home {
        Some(h) => h,
        None => warn_exit(b"can't change directory to home."),
    };
    if !chdir(&home) {
        warn_exit3("can't change directory to ", &home, ".");
    }
}

/****************************************************************
 *
 * header sub functions
 *
 */

/*
 * p:            a point to be scanned in the buffer.
 * lim:          the end of the buffer.
 * hdr_only:     if YES, scan the header only (not the body).
 *
 * return value: the beginning point of the next line. None is
 *               returned if either the buffer or the header ends.
 */
fn nextline(buf: &mut [u8], p: usize, lim: usize, hdr_only: bool) -> Option<usize> {
    if p == lim {
        return None;
    }
    if buf[p] == LF && hdr_only {
        return None;
    }
    let mut p = p;
    while p < lim {
        if buf[p] == LF {
            break;
        }
        p += 1;
    }
    p += 1;
    if p < lim {
        Some(p)
    } else {
        buf[lim - 1] = LF;
        None
    }
}

/*
 * p:             a point to be scanned in the buffer.
 * lim:           the end of the buffer.
 * field_len:     if the number of the lines of the field is over
 *                field_len, it is truncated. And the end point is
 *                returned as truncated_end.
 * hdr_only:      if YES, scan the header only (not the body).
 *
 * return value:  (a point of the next field, truncated_end,
 *                the beginning point of the last line of the field).
 *                The first is None if either the buffer or the
 *                header ends. The last is used for the References: field.
 */
fn nextfield(
    buf: &mut [u8],
    p: usize,
    lim: usize,
    field_len: i32,
    hdr_only: bool,
) -> (Option<usize>, Option<usize>, usize) {
    let mut i: i32 = 0;
    let mut truncated_end = None;
    let mut p = p;
    loop {
        let q = p;
        match nextline(buf, p, lim, hdr_only) {
            None => return (None, truncated_end, q),
            Some(n) => {
                i = i.wrapping_add(1);
                if field_len != 0 && i == field_len {
                    truncated_end = Some(n);
                }
                if buf[n] != SP && buf[n] != TAB {
                    return (Some(n), truncated_end, q);
                }
                p = n;
            }
        }
    }
}

fn eoh(buf: &[u8], p: usize, lim: usize) -> isize {
    let mut p = p;
    while p < lim {
        if buf[p] == LF && p + 1 < lim && buf[p + 1] == LF {
            return (p + 1) as isize;
        }
        p += 1;
    }
    lim as isize - 1
}

/*
 * The C string starting at beg when NUL is put at nul_at
 * temporarily.
 */
fn cstr(buf: &[u8], beg: usize, nul_at: isize) -> &[u8] {
    if beg >= buf.len() {
        return &[];
    }
    let lim = if nul_at >= beg as isize {
        (nul_at as usize).min(buf.len())
    } else {
        buf.len()
    };
    let s = &buf[beg..lim];
    match s.iter().position(|&c| c == NUL) {
        Some(i) => &s[..i],
        None => s,
    }
}

/****************************************************************
 *
 * String search
 *
 */

/*
 * mystrcasechr() of mewl compares a char with tolower(c) and toupper(c).
 * When char is signed, glibc's tolower() and toupper() map -128..-1
 * to 128..255, so the first character of the value >= 0x80 never
 * matches.
 */
const HIGH_FIRST_CHAR_NEVER_MATCHES: bool =
    cfg!(all(target_os = "linux", target_env = "gnu")) && (std::ffi::c_char::MIN != 0);

/* strstr() and mystrcasestr() */
fn search_in(hay: &[u8], needle: &[u8], case_sensitive: bool) -> bool {
    if needle.is_empty() {
        return true;
    }
    if case_sensitive {
        hay.windows(needle.len()).any(|w| w == needle)
    } else {
        if HIGH_FIRST_CHAR_NEVER_MATCHES && needle[0] >= 0x80 {
            return false;
        }
        hay.windows(needle.len()).any(|w| w.eq_ignore_ascii_case(needle))
    }
}

fn ends_with(s: &[u8], suffix: &[u8]) -> bool {
    s.ends_with(suffix)
}

/****************************************************************
 *
 * parser for folder and range
 *
 */

fn set_fld_rng(arg1: Option<&[u8]>, arg2: Option<&[u8]>) -> FldRng {
    let mut folder = arg1;
    let mut range = None;

    if let Some(a) = arg1 {
        if let Some(i) = a.iter().position(|&c| c == SP) {
            folder = Some(&a[..i]);
            range = Some(&a[i + 1..]);
        }
    }
    if range.is_none() {
        range = arg2;
    }

    let (first, last, num) = match range {
        None => (0, 0, 0),
        Some(r) if r.starts_with(b"last:") => (0, 0, atoi(&r[5..]) as u32),
        Some(r) => match r.iter().position(|&c| c == b'-') {
            None => {
                let n = atoi(r) as u32;
                (n, n, 0)
            }
            Some(i) => (atoi(&r[..i]) as u32, atoi(&r[i + 1..]) as u32, 0),
        },
    };

    let folder = folder.unwrap_or(INBOX);
    let folder = folder
        .iter()
        .map(|&c| if c == FILE_SEPARATOR { SP } else { c })
        .collect();

    FldRng {
        folder,
        first,
        last,
        num,
    }
}

impl Mewl {
    /****************************************************************
     *
     * folder function
     *
     */

    fn ch_mail_home(&self) {
        let dir = &self.mail_home;
        match dir.first() {
            Some(&b'~') => {
                ch_home();
                let p = &dir[1..];
                if p.is_empty() {
                    return;
                }
                if p[0] != FILESEP {
                    warn_exit3("can't change directory to ", dir, ".");
                }
                let p = &p[1..];
                if p.is_empty() {
                    return;
                }
                if !chdir(p) {
                    warn_exit3("can't change directory to ", dir, ".");
                }
            }
            Some(&FILESEP) => {
                if !chdir(dir) {
                    warn_exit3("can't change directory to ", dir, ".");
                }
            }
            _ => {
                ch_home();
                if !chdir(dir) {
                    warn_exit3("can't change directory to ", dir, ".");
                }
            }
        }
    }

    fn ch_folder(&mut self, folder: &[u8]) {
        match folder.first() {
            Some(&b'+') => {
                self.ch_mail_home();
                let p = &folder[1..];
                if p.is_empty() {
                    return;
                }
                if !chdir(p) {
                    if self.no_fld_flag {
                        self.default_fld_error_flag = true;
                    } else {
                        warn_exit3("can't change folder to ", folder, ".");
                    }
                }
            }
            Some(&b'~') => {
                ch_home();
                let p = &folder[1..];
                if p.is_empty() {
                    return;
                }
                if p[0] != FILESEP {
                    warn_exit3("can't change folder to ", folder, ".");
                }
                let p = &p[1..];
                if p.is_empty() {
                    return;
                }
                if !chdir(p) {
                    warn_exit3("can't change folder to ", folder, ".");
                }
            }
            _ => {
                if !chdir(folder) {
                    warn_exit3("can't change folder to ", folder, ".");
                }
            }
        }
    }

    /****************************************************************
     *
     * print sub-functions
     *
     */

    fn init_fields(&mut self, fields: Option<&[u8]>) {
        let fields = fields.unwrap_or(DEFAULT_FIELDS);
        /* strdup() and strlen() stop at NUL. */
        let fields = fields.split(|&c| c == NUL).next().unwrap_or(&[]);
        self.fields = fields.split(|&c| c == b',').map(|f| f.to_vec()).collect();
        self.ref_idx = self.fields.iter().rposition(|f| f == REFERENCES);
    }

    fn print_field(&mut self, lim: usize, fname: &[u8], fld: &[u8]) {
        let nfld = self.fields.len();
        let mut beg: Vec<Option<usize>> = vec![None; nfld];
        let mut end: Vec<usize> = vec![0; nfld];
        let mut body_beg = None;
        let limp = lim;
        let mut p = 0;

        loop {
            let matched = self
                .fields
                .iter()
                .position(|f| strncasecmp_at(&self.buf, p, f));

            let next;
            if let Some(i) = matched {
                beg[i] = Some(p);
                let (np, tend, prev) = nextfield(&mut self.buf, p, limp, self.field_len, false);
                next = np;

                if Some(i) == self.ref_idx {
                    /* For the References: field, the bottom
                    part of its value is important. */
                    if beg[i] == Some(prev) {
                        /* Skip the field name since
                        it will be printed below. */
                        beg[i] = Some(p + self.fields[i].len());
                    } else {
                        /* The last line only */
                        beg[i] = Some(prev);
                    }
                    end[i] = np.unwrap_or(limp);
                } else {
                    end[i] = tend.or(np).unwrap_or(limp);
                }
            } else {
                /* not a target field. let's skip this */
                next = nextfield(&mut self.buf, p, limp, self.field_len, false).0;
            }

            match next {
                None => break,
                Some(n) => p = n,
            }

            if self.buf[p] == LF {
                /* header ends */
                body_beg = Some(p + 1);
                break;
            }
        }

        out(b"Folder: ");
        out(fld);
        out(b"\n");
        out(b"Filename: ");
        out(fname);
        out(b"\n");
        for i in 0..nfld {
            if let Some(b) = beg[i] {
                if Some(i) == self.ref_idx {
                    out(REFERENCES);
                }
                if b < end[i] {
                    out(&self.buf[b..end[i]]);
                }
            }
        }
        out(b"\n");
        if let Some(bb) = body_beg {
            let mut p = bb;
            let mut i: u32 = 0;
            while i < self.body_len as u32 {
                let q = p;
                match nextline(&mut self.buf, q, limp, false) {
                    None => {
                        if q < limp {
                            out(&self.buf[q..limp]);
                        }
                        break;
                    }
                    Some(n) => {
                        p = n;
                        i += 1;
                        if self.buf[q] == b'.' {
                            out(b".");
                        }
                        out(&self.buf[q..p]);
                    }
                }
            }
        }
        out(b".\n");
    }

    fn search_string(&mut self, key: &[u8], value: &[u8], case_sensitive: bool) -> bool {
        let limp = self.search_lim;

        if key == HEAD {
            let end = eoh(&self.buf, 0, limp);
            return search_in(cstr(&self.buf, 0, end), value, case_sensitive);
        }

        let mut p = 0;
        loop {
            let next;
            if strncasecmp_at(&self.buf, p, key) {
                let beg = p + key.len();
                let (np, tend, _) = nextfield(&mut self.buf, p, limp, self.field_len, true);
                next = np;
                let end = match (tend, np) {
                    (Some(t), _) => t as isize - 1, /* end == LF */
                    (None, None) => limp as isize - 1, /* end == LF, see nextline */
                    (None, Some(n)) => n as isize - 1, /* end == LF */
                };
                if search_in(cstr(&self.buf, beg, end), value, case_sensitive) {
                    return true;
                }
            } else {
                next = nextfield(&mut self.buf, p, limp, self.field_len, true).0;
            }
            match next {
                None => break,
                Some(n) => p = n,
            }
        }
        false
    }

    /****************************************************************
     *
     * print functions
     *
     */

    fn print_for_scan(&mut self, fp: &mut File, fname: &[u8], fld: &[u8]) -> bool {
        let lim = fread(fp, &mut self.buf);
        self.print_field(lim, fname, fld);
        true
    }

    fn print_for_sort(&mut self, fp: &mut File, fname: &[u8], _fld: &[u8]) -> bool {
        let lim = fread(fp, &mut self.buf);
        let limp = lim;
        let len = self.key_field.len();
        let mut beg = None;
        let mut end = 0;
        let mut p = 0;

        loop {
            if strncasecmp_at(&self.buf, p, &self.key_field) {
                beg = Some(p + len);
                let (np, tend, _) = nextfield(&mut self.buf, p, limp, self.field_len, true);
                end = match (tend, np) {
                    (Some(t), _) => t - 1,
                    (None, None) => limp,
                    (None, Some(n)) => n - 1,
                };
                break;
            }
            match nextfield(&mut self.buf, p, limp, self.field_len, true).0 {
                None => break,
                Some(n) => p = n,
            }
        }

        match beg {
            None => {
                out(fname);
                out(b": \n");
            }
            Some(b) => {
                out(fname);
                out(b":");
                if b < end {
                    out(&self.buf[b..end]);
                }
                out(b"\n");
            }
        }

        false
    }

    fn print_for_pick(&mut self, fp: &mut File, fname: &[u8], fld: &[u8]) -> bool {
        let lim = fread(fp, &mut self.buf);

        self.search_lim = lim;
        let pattern = self.pattern.take().unwrap();
        let matched = pattern.matches(&mut |k, v, cs| self.search_string(k, v, cs));
        self.pattern = Some(pattern);
        if matched {
            if self.all_fields > 0 {
                self.print_field(lim, fname, fld);
                return true;
            } else {
                out(fname);
                out(b"\n");
                return false;
            }
        }
        false
    }

    /****************************************************************
     *
     * scan functions
     *
     */

    fn scan_numfile(&self, s: &[u8], first: u32, last: u32) -> bool {
        let mut num: i32 = 0;
        let mut i = 0;

        while i < s.len() && s[i].is_ascii_digit() {
            num = num.wrapping_mul(10).wrapping_add((s[i] - b'0') as i32);
            i += 1;
        }

        let rest = &s[i..];
        if num == 0 {
            return false;
        } else if rest.is_empty() || rest == &self.suffix[..] {
        } else {
            return false;
        }

        let num = num as u32;
        num >= first && (last == 0 || num <= last)
    }

    fn scan_folder(&mut self, j: usize) -> usize {
        let folder = self.folders[j].folder.clone();
        let (first, last, fnum) = (self.folders[j].first, self.folders[j].last, self.folders[j].num);

        self.ch_folder(&folder);

        let dirp = match std::fs::read_dir(".") {
            Ok(d) => d,
            Err(_) => warn_exit3("can't open folder: ", &folder, "."),
        };

        self.scan_buf.clear();
        for dp in dirp {
            let dp = match dp {
                Ok(dp) => dp,
                Err(_) => break,
            };
            let fname = dp.file_name();
            let fname = fname.as_bytes();
            if self.scan_numfile(fname, first, last) {
                self.scan_buf.push(atoi(fname) as u32);
            }
        }

        self.scan_buf.sort_unstable();
        let mut num = self.scan_buf.len();

        if fnum > 0 && (fnum as usize) < num {
            self.scan_buf.drain(..num - fnum as usize);
            num = fnum as usize;
        }

        if self.print_num_of_msg && num > 0 && self.scan_folders == 1 {
            out(format!("NumOfMsg: {}\n", num as i32).as_bytes());
            out_flush();
        }
        num
    }

    fn scan_init(&mut self, folders: usize) {
        self.scan_folders = folders;
        self.scan_messages = 0;
    }

    fn scan_getfile(&mut self) -> Option<(File, Vec<u8>, Vec<u8>)> {
        loop {
            if self.scan_i == self.scan_messages {
                if self.scan_j < self.scan_folders {
                    let j = self.scan_j;
                    self.scan_j += 1;
                    self.scan_messages = self.scan_folder(j);
                    self.scan_i = 0;
                } else {
                    return None;
                }
            }

            let foldername = &self.folders[self.scan_j - 1].folder;

            while self.scan_i < self.scan_messages {
                let n = self.scan_buf[self.scan_i] as i32;
                for zero in ["", "0"] {
                    let name = format!("{}{}", zero, n).into_bytes();
                    let mut name2 = name.clone();
                    name2.extend_from_slice(&self.suffix);
                    if let Some(fp) = fopen(&name2).or_else(|| fopen(&name)) {
                        self.scan_i += 1;
                        return Some((fp, name, foldername.clone()));
                    }
                }
                self.scan_i += 1;
            }
        }
    }

    /****************************************************************
     *
     * exec functions
     *
     */

    fn exec_init(&mut self, _num: usize) {
        if self.folders[0].folder.len() >= MAX_FILE_NAME_LEN {
            warn_exit(b"folder name is too long.");
        }
        self.current_folder = self.folders[0].folder.clone();
        let folder = self.current_folder.clone();
        self.ch_folder(&folder);

        let mut argvec: Vec<Vec<u8>> = Vec::new();
        if let Some(arg) = &self.exec_arg {
            argvec.extend(arg.splitn(MAX_ARGC - 3, |&c| c == SP).map(|a| a.to_vec()));
        }
        argvec.push(self.current_folder.clone());

        let child = Command::new(OsStr::from_bytes(&self.exec_cmd))
            .args(argvec.iter().map(|a| OsStr::from_bytes(a)))
            .stdout(Stdio::piped())
            .spawn();
        let input: Box<dyn BufRead> = match child {
            Ok(mut c) => Box::new(BufReader::new(c.stdout.take().unwrap())),
            Err(_) => {
                /* The child of mewl reports this and exits. */
                let mut msg = b"'".to_vec();
                msg.extend_from_slice(&self.exec_cmd);
                msg.extend_from_slice(b"' command not found.");
                warning(&msg);
                Box::new(io::empty())
            }
        };
        self.input = Some(input);
    }

    fn exec_getfile(&mut self) -> Option<(File, Vec<u8>, Vec<u8>)> {
        loop {
            let input = self.input.as_mut().unwrap();
            let line = fgets(input.as_mut(), MAX_FILE_NAME_LEN)?;
            /* strchr() stops at NUL. */
            let s = match line.iter().position(|&c| c == NUL) {
                Some(i) => &line[..i],
                None => &line[..],
            };
            let s = match s.iter().position(|&c| c == LF) {
                Some(i) => s[..i].to_vec(),
                None => {
                    skip_line(input.as_mut());
                    s.to_vec()
                }
            };

            if s.starts_with(CHDIR) {
                self.no_fld_flag = false;
                self.default_fld_error_flag = false;
                let p = &s[CHDIR.len()..];
                let p = &p[p.iter().position(|&c| c != SP && c != TAB).unwrap_or(p.len())..];
                if p.len() >= MAX_FILE_NAME_LEN {
                    warn_exit(b"folder name is too long.");
                }
                self.current_folder = p.to_vec();
                let folder = self.current_folder.clone();
                self.ch_folder(&folder);
                continue;
            }
            if self.default_fld_error_flag {
                warn_exit(b"default folder is not exist.");
            }
            let p = &s[s.iter().position(|&c| c != SP && c != TAB).unwrap_or(s.len())..];
            let stripped = if ends_with(p, &self.suffix) {
                &p[..p.len() - self.suffix.len()]
            } else {
                p
            };
            if let Some(fp) = fopen(p) {
                return Some((fp, stripped.to_vec(), self.current_folder.clone()));
            }
            /* try again without suffix */
            if let Some(fp) = fopen(stripped) {
                return Some((fp, stripped.to_vec(), self.current_folder.clone()));
            }
        }
    }

    /****************************************************************
     *
     * stdin functions
     *
     */

    fn stdin_init(&mut self, _num: usize) {
        let input: Box<dyn BufRead> = if self.input_file != STDINFILE {
            match fopen(&self.input_file) {
                Some(f) => Box::new(BufReader::new(f)),
                None => warn_exit3("can't open ", &self.input_file, "."),
            }
        } else {
            Box::new(BufReader::new(io::stdin()))
        };
        self.input = Some(input);

        if self.folders[0].folder.len() >= MAX_FILE_NAME_LEN {
            warn_exit(b"folder name is too long.");
        }
        self.current_folder = self.folders[0].folder.clone();
        let folder = self.current_folder.clone();
        self.ch_folder(&folder);
    }
}

/****************************************************************
 *
 * main
 *
 */

fn main() {
    /* Like C, die by SIGPIPE when the reader goes away. */
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }

    let argv: Vec<Vec<u8>> = std::env::args_os().map(|a| a.into_vec()).collect();
    let progname = getprognm(argv.first().map(|a| &a[..]).unwrap_or(b""));
    set_warn_prog(&progname);

    let mut m = Mewl {
        buf: vec![0; MAX_HEADER],
        mail_home: MAILHOME.to_vec(),
        current_folder: Vec::new(),
        folders: Vec::new(),
        key_field: Vec::new(),
        exec_cmd: Vec::new(),
        exec_arg: None,
        input_file: Vec::new(),
        suffix: Vec::new(),
        all_fields: 0,
        field_len: MAX_FIELD_LEN,
        body_len: MAX_BODY_LEN,
        print_num_of_msg: false,
        no_fld_flag: false,
        default_fld_error_flag: false,
        fields: Vec::new(),
        ref_idx: None,
        pattern: None,
        search_lim: 0,
        scan_folders: 0,
        scan_messages: 0,
        scan_buf: Vec::new(),
        scan_i: 0,
        scan_j: 0,
        input: None,
    };

    let (mut eflag, mut nflag, mut sflag, mut wflag) = (0, 1, 0, 0);
    let mut key_field = None;
    let mut input_file = None;
    let mut suffix = None;
    let mut fields = None;
    let mut pattern = None;

    let mut g = Getopt::new();
    while let Some(optc) = g.getopt(&argv, b"ab:c:d:e:f:hi:l:m:np:s:vwx:") {
        let optarg = g.optarg.clone();
        match optc {
            b'a' => m.all_fields += 1,
            b'b' => m.mail_home = optarg,
            b'c' => m.body_len = atoi(&optarg),
            b'd' => key_field = Some(optarg),
            b'e' => {
                m.exec_cmd = optarg;
                eflag += 1;
            }
            b'f' => {
                fields = Some(optarg);
                m.print_num_of_msg = true;
            }
            b'h' => {
                help(&progname);
                exit(libc::EXIT_SUCCESS);
            }
            b'i' => input_file = Some(optarg),
            b'l' => m.field_len = atoi(&optarg),
            b'm' => m.exec_arg = Some(optarg),
            b'n' => nflag = 0,
            b'p' => pattern = Some(optarg),
            b's' => {
                if m.folders.len() >= MAX_FOLDER {
                    warn_exit(b"too many folders.");
                }
                m.folders.push(set_fld_rng(Some(&optarg), None));
                sflag += 1;
            }
            b'v' => {
                version(&progname);
                exit(libc::EXIT_SUCCESS);
            }
            b'w' => wflag += 1,
            b'x' => suffix = Some(optarg),
            _ => {
                usage(&progname);
                exit(libc::EXIT_FAILURE);
            }
        }
    }

    let rest = argv.len() - g.optind;
    let args = &argv[g.optind..];

    /* Suffix */
    m.suffix = suffix.unwrap_or_else(|| SUFFIX.to_vec());

    if sflag == 0 {
        match rest {
            0 => {
                m.folders.push(set_fld_rng(None, None));
                m.no_fld_flag = true;
            }
            1 => m.folders.push(set_fld_rng(Some(&args[0]), None)),
            2 => m.folders.push(set_fld_rng(Some(&args[0]), Some(&args[1]))),
            _ => {
                usage(&progname);
                exit(libc::EXIT_FAILURE);
            }
        }
    } else if rest != 0 {
        warn_exit(b"the 's' option can't co-exist with arguments.");
    }
    let lim = m.folders.len();

    let source = if let Some(f) = input_file {
        m.input_file = f;
        Source::Stdin
    } else if eflag > 0 {
        Source::Exec
    } else {
        Source::Scan
    };

    m.init_fields(fields.as_deref()); /* lengthy sometime */

    let printer = if let Some(p) = pattern {
        m.pattern = Some(Pattern::new(&p));
        m.print_num_of_msg = false;
        Printer::Pick
    } else if let Some(k) = key_field {
        m.key_field = k;
        Printer::Sort
    } else {
        Printer::Scan
    };

    /* dup2(READ, SYNC); sync = fdopen(SYNC, FDREAD); */
    let mut sync = if unsafe { libc::dup2(0, 3) } == 3 {
        Some(BufReader::new(unsafe { File::from_raw_fd(3) }))
    } else {
        None
    };

    match source {
        Source::Stdin => m.stdin_init(lim),
        Source::Exec => m.exec_init(lim),
        Source::Scan => m.scan_init(lim),
    }

    let mut st_mode: u32 = 0;
    loop {
        let got = match source {
            Source::Scan => m.scan_getfile(),
            _ => m.exec_getfile(),
        };
        let (mut fp, filename, foldername) = match got {
            Some(g) => g,
            None => break,
        };
        if nflag != 0 {
            if let Ok(st) = fp.metadata() {
                st_mode = st.mode();
            }
            if st_mode & S_IFREG == 0 {
                continue;
            }
        }
        let wait = match printer {
            Printer::Scan => m.print_for_scan(&mut fp, &filename, &foldername),
            Printer::Sort => m.print_for_sort(&mut fp, &filename, &foldername),
            Printer::Pick => m.print_for_pick(&mut fp, &filename, &foldername),
        };
        if wait && wflag > 0 {
            if let Some(s) = sync.as_mut() {
                out_flush();
                let mut c = [0u8; 1];
                let _ = s.read(&mut c);
            }
        }
    }

    exit(libc::EXIT_SUCCESS);
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
 * mewl.rs ends here
 */
