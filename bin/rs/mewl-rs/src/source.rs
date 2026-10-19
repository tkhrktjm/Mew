//! Where messages come from.
//!
//! - [`FolderScanner`]: message files in folders (default).
//! - [`ListReader`]: file names read from a file or stdin (`-i`),
//!   or from the output of an external command (`-e`).

use std::ffi::OsStr;
use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, Read};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::MetadataExt;
use std::process::{Command, Stdio};

use crate::compat::atoi;
use crate::error::{self, Result, fatal};
use crate::folder::{self, Entered, FolderRange};
use crate::output::Output;

const MAX_FILE_NAME_LEN: usize = libc::PATH_MAX as usize;

/// `-m` is split into at most this many arguments.
const MAX_EXEC_ARGS: usize = 13;

/// A line of `-i` or `-e` to change the folder: `CD: folder`.
const CHDIR: &[u8] = b"CD:";

const STDIN: &[u8] = b"-";

/// The `S_IFREG` bit of `st_mode`.
const S_IFREG: u32 = 0o100000;

pub struct Message {
    pub file: File,
    /// The message number without the suffix.
    pub name: Vec<u8>,
    pub folder: Vec<u8>,
}

impl Message {
    /// mewl tests the `S_IFREG` bit, not the file type, and keeps the
    /// previous mode if `fstat()` fails.
    pub fn is_regular(&self, mode: &mut u32) -> bool {
        if let Ok(meta) = self.file.metadata() {
            *mode = meta.mode();
        }
        *mode & S_IFREG != 0
    }
}

pub enum Source {
    Folders(FolderScanner),
    List(ListReader),
}

impl Source {
    pub fn next_message(&mut self, out: &mut Output) -> Result<Option<Message>> {
        match self {
            Source::Folders(s) => s.next_message(out),
            Source::List(s) => s.next_message(),
        }
    }
}

fn open(name: &[u8]) -> Option<File> {
    File::open(OsStr::from_bytes(name)).ok()
}

fn check_folder_name(folder: &[u8]) -> Result<()> {
    if folder.len() >= MAX_FILE_NAME_LEN {
        return Err(fatal!("folder name is too long."));
    }
    Ok(())
}

/****************************************************************
 *
 * Folders
 *
 */

pub struct FolderScanner {
    ranges: std::vec::IntoIter<FolderRange>,
    folder: Vec<u8>,
    /// The message numbers in the current folder.
    numbers: std::vec::IntoIter<u32>,
    mail_home: Vec<u8>,
    suffix: Vec<u8>,
    is_default: bool,
    /// Print `NumOfMsg:` before the messages.
    print_count: bool,
}

impl FolderScanner {
    pub fn new(
        ranges: Vec<FolderRange>,
        mail_home: Vec<u8>,
        suffix: Vec<u8>,
        is_default: bool,
        print_count: bool,
    ) -> Self {
        FolderScanner {
            print_count: print_count && ranges.len() == 1,
            ranges: ranges.into_iter(),
            folder: Vec::new(),
            numbers: Vec::new().into_iter(),
            mail_home,
            suffix,
            is_default,
        }
    }

    fn next_message(&mut self, out: &mut Output) -> Result<Option<Message>> {
        loop {
            for num in self.numbers.by_ref() {
                /* "%d" of unsigned int */
                let num = num as i32;
                for name in [format!("{num}"), format!("0{num}")] {
                    let name = name.into_bytes();
                    let with_suffix = [&name[..], &self.suffix].concat();
                    if let Some(file) = open(&with_suffix).or_else(|| open(&name)) {
                        return Ok(Some(Message { file, name, folder: self.folder.clone() }));
                    }
                }
            }
            let Some(range) = self.ranges.next() else {
                return Ok(None);
            };
            self.scan_folder(range, out)?;
        }
    }

    /// Lists the messages in a folder.
    fn scan_folder(&mut self, range: FolderRange, out: &mut Output) -> Result<()> {
        /* A missing default folder leaves us in the Mail home. */
        folder::change_to_folder(&range.folder, &self.mail_home, self.is_default)?;

        let entries = fs::read_dir(".").map_err(|_| fatal!("can't open folder: ", range.folder, "."))?;
        let mut numbers: Vec<u32> = entries
            .map_while(|entry| entry.ok())
            .filter_map(|entry| {
                let name = entry.file_name();
                let name = name.as_bytes();
                let num = self.message_number(name)?;
                range.contains(num).then(|| atoi(name) as u32)
            })
            .collect();
        numbers.sort_unstable();

        let count = range.count as usize;
        if count > 0 && count < numbers.len() {
            numbers.drain(..numbers.len() - count);
        }

        if self.print_count && !numbers.is_empty() {
            out.put(format!("NumOfMsg: {}\n", numbers.len()).as_bytes()).flush();
        }

        self.folder = range.folder;
        self.numbers = numbers.into_iter();
        Ok(())
    }

    /// The number of a message file: digits optionally followed by
    /// the suffix. It overflows like int in C.
    fn message_number(&self, name: &[u8]) -> Option<u32> {
        let digits = name.iter().take_while(|c| c.is_ascii_digit()).count();
        let num = name[..digits]
            .iter()
            .fold(0i32, |n, &c| n.wrapping_mul(10).wrapping_add(i32::from(c - b'0')));
        let rest = &name[digits..];
        (num != 0 && (rest.is_empty() || rest == self.suffix)).then_some(num as u32)
    }
}

/****************************************************************
 *
 * File names from a file, stdin, or a command
 *
 */

pub struct ListReader {
    input: Box<dyn BufRead>,
    folder: Vec<u8>,
    mail_home: Vec<u8>,
    suffix: Vec<u8>,
    /// No folder is specified and no `CD:` has been read.
    is_default: bool,
    default_missing: bool,
}

impl ListReader {
    /// `-i file`, `-` is stdin.
    pub fn open(path: &[u8], folder: &[u8], mail_home: Vec<u8>, suffix: Vec<u8>, is_default: bool) -> Result<Self> {
        let input: Box<dyn BufRead> = if path == STDIN {
            Box::new(BufReader::new(io::stdin()))
        } else {
            let file = open(path).ok_or_else(|| fatal!("can't open ", path, "."))?;
            Box::new(BufReader::new(file))
        };
        Self::new(input, folder, mail_home, suffix, is_default)
    }

    /// `-e command [-m args]`: runs `command args... folder` in the
    /// folder and reads its stdout. `args` is split by spaces.
    pub fn spawn(
        command: &[u8],
        args: Option<&[u8]>,
        folder: &[u8],
        mail_home: Vec<u8>,
        suffix: Vec<u8>,
        is_default: bool,
    ) -> Result<Self> {
        let mut reader = Self::new(Box::new(io::empty()), folder, mail_home, suffix, is_default)?;

        let args = args.into_iter().flat_map(|a| a.splitn(MAX_EXEC_ARGS, |&c| c == b' '));
        let child = Command::new(OsStr::from_bytes(command))
            .args(args.map(OsStr::from_bytes))
            .arg(OsStr::from_bytes(folder))
            .stdout(Stdio::piped())
            .spawn();
        match child {
            Ok(mut child) => {
                let stdout = child.stdout.take().expect("stdout is piped");
                reader.input = Box::new(BufReader::new(stdout));
            }
            /* In mewl, the forked child reports this and exits, and
            the parent sees nothing. */
            Err(_) => error::warn(&[b"'", command, b"' command not found."].concat()),
        }
        Ok(reader)
    }

    fn new(input: Box<dyn BufRead>, folder: &[u8], mail_home: Vec<u8>, suffix: Vec<u8>, is_default: bool) -> Result<Self> {
        check_folder_name(folder)?;
        let mut reader = ListReader {
            input,
            folder: folder.to_vec(),
            mail_home,
            suffix,
            is_default,
            default_missing: false,
        };
        reader.change_folder()?;
        Ok(reader)
    }

    fn change_folder(&mut self) -> Result<()> {
        let entered = folder::change_to_folder(&self.folder, &self.mail_home, self.is_default)?;
        self.default_missing = entered == Entered::MissingDefault;
        Ok(())
    }

    fn next_message(&mut self) -> Result<Option<Message>> {
        while let Some(line) = self.read_line() {
            if let Some(folder) = line.strip_prefix(CHDIR) {
                self.is_default = false;
                let folder = folder.trim_ascii_start_blank();
                check_folder_name(folder)?;
                self.folder = folder.to_vec();
                self.change_folder()?;
                continue;
            }
            if self.default_missing {
                return Err(fatal!("default folder is not exist."));
            }
            let name = line.trim_ascii_start_blank();
            let stem = name.strip_suffix(&self.suffix[..]).unwrap_or(name);
            /* The name with the suffix first, then without it. */
            if let Some(file) = open(name).or_else(|| open(stem)) {
                return Ok(Some(Message { file, name: stem.to_vec(), folder: self.folder.clone() }));
            }
        }
        Ok(None)
    }

    /// A line like `fgets()` with a buffer of `MAX_FILE_NAME_LEN`:
    /// the rest of a long line is discarded. A line ends at NUL, too.
    fn read_line(&mut self) -> Option<Vec<u8>> {
        let limit = MAX_FILE_NAME_LEN - 1;
        let mut chunk = Vec::new();
        let _ = (&mut self.input).take(limit as u64).read_until(b'\n', &mut chunk);
        if chunk.is_empty() {
            return None;
        }
        /* A line is a C string: it ends at NUL or LF. */
        let line_end = chunk.iter().position(|&c| c == b'\n' || c == 0);
        if line_end.is_none_or(|i| chunk[i] == 0) {
            /* No LF seen: discard the rest of the line. */
            let _ = self.input.skip_until(b'\n');
        }
        chunk.truncate(line_end.unwrap_or(chunk.len()));
        Some(chunk)
    }
}

trait TrimBlank {
    /// Skips leading spaces and tabs.
    fn trim_ascii_start_blank(&self) -> &[u8];
}

impl TrimBlank for [u8] {
    fn trim_ascii_start_blank(&self) -> &[u8] {
        let n = self.iter().take_while(|&&c| c == b' ' || c == b'\t').count();
        &self[n..]
    }
}
