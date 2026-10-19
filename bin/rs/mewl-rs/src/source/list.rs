//! File names from a file, stdin, or a command.
//!
//! Each line is the name of a message file in the current folder, or
//! `CD: folder` to change the folder.

use std::ffi::OsStr;
use std::io::{self, BufRead, BufReader, Read};
use std::os::unix::ffi::OsStrExt;
use std::process::{Command, Stdio};

use super::{Entry, MAX_FILE_NAME_LEN, Message, Settings, check_folder_name, open};
use crate::bytes::BytesExt;
use crate::error::{Error, Reporter, Result};
use crate::folder::Entered;

/// `-m` is split into at most this many arguments.
const MAX_EXEC_ARGS: usize = 13;

const CHDIR: &[u8] = b"CD:";

const STDIN: &[u8] = b"-";

pub struct ListReader {
    input: Box<dyn BufRead>,
    folder: Vec<u8>,
    settings: Settings,
    /// The default folder does not exist, which is an error when a
    /// message is read before `CD:`.
    default_missing: bool,
}

impl ListReader {
    /// `-i path`, `-` is stdin.
    pub fn open(path: &[u8], folder: &[u8], settings: Settings) -> Result<Self> {
        let input: Box<dyn BufRead> = if path == STDIN {
            Box::new(BufReader::new(io::stdin()))
        } else {
            let file = open(path).ok_or_else(|| Error::OpenFile(path.to_vec()))?;
            Box::new(BufReader::new(file))
        };
        Self::new(input, folder, settings)
    }

    /// `-e command [-m args]`: runs `command args... folder` in the
    /// folder and reads its stdout. `args` is split by spaces.
    ///
    /// If the command cannot be run, that is reported and nothing is
    /// read: in mewl, the forked child reports it and exits.
    pub fn spawn(
        command: &[u8],
        args: Option<&[u8]>,
        folder: &[u8],
        settings: Settings,
        reporter: &Reporter,
    ) -> Result<Self> {
        let mut reader = Self::new(Box::new(io::empty()), folder, settings)?;

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
            Err(_) => reporter.warn(&Error::CommandNotFound(command.to_vec())),
        }
        Ok(reader)
    }

    fn new(input: Box<dyn BufRead>, folder: &[u8], settings: Settings) -> Result<Self> {
        check_folder_name(folder)?;
        let mut reader = ListReader { input, folder: folder.to_vec(), settings, default_missing: false };
        reader.enter_folder()?;
        Ok(reader)
    }

    fn enter_folder(&mut self) -> Result<()> {
        let entered = self.settings.navigator.enter(&self.folder, self.settings.is_default)?;
        self.default_missing = entered == Entered::MissingDefault;
        Ok(())
    }

    fn next_message(&mut self) -> Result<Option<Message>> {
        while let Some(line) = self.read_line() {
            if let Some(folder) = line.strip_prefix(CHDIR) {
                let folder = folder.trim_start_blank();
                check_folder_name(folder)?;
                self.folder = folder.to_vec();
                self.settings.is_default = false;
                self.enter_folder()?;
                continue;
            }
            if self.default_missing {
                return Err(Error::DefaultFolderMissing);
            }
            let name = line.trim_start_blank();
            let stem = name.strip_suffix(&self.settings.suffix[..]).unwrap_or(name);
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
        let rest_left = if self.settings.flavor.fixes_bugs() {
            !chunk.ends_with(b"\n") && chunk.len() == limit
        } else {
            /* mewl looks for LF only before NUL, so a line with NUL
            makes it discard the next line. */
            line_end.is_none_or(|i| chunk[i] == 0)
        };
        if rest_left {
            /* No LF seen: discard the rest of the line. */
            let _ = self.input.skip_until(b'\n');
        }
        chunk.truncate(line_end.unwrap_or(chunk.len()));
        Some(chunk)
    }
}

impl Iterator for ListReader {
    type Item = Result<Entry>;

    fn next(&mut self) -> Option<Self::Item> {
        self.next_message().map(|m| m.map(Entry::Message)).transpose()
    }
}
