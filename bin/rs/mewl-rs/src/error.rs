//! Errors and diagnostics.
//!
//! mewl reports a fatal error as `progname: message` on stderr and
//! exits with 1 (`warn_exit()`). Here an error is propagated up to the
//! caller of [`crate::main`] as [`Error`], and [`Reporter`] prints it.

use std::fmt;
use std::io::{self, Write};

use crate::pattern::PatternError;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    /// Reported with the usage line.
    Usage,
    /// An option with more than one letter.
    UnknownLongOption(Vec<u8>),
    UnknownOption(u8),
    NoParameter(u8),
    TooManyFolders,
    /// `-s` with folder operands.
    SourceWithOperands,
    NoHomeDirectory,
    HomeUnknown,
    ChangeDirectory(Vec<u8>),
    ChangeFolder(Vec<u8>),
    OpenFolder(Vec<u8>),
    OpenFile(Vec<u8>),
    FolderNameTooLong,
    DefaultFolderMissing,
    /// Not fatal: mewl goes on without the command (`-e`).
    CommandNotFound(Vec<u8>),
    Pattern(PatternError),
}

pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    /// The message as mewl prints it. File and folder names are not
    /// necessarily UTF-8, so it is made of bytes.
    pub fn message(&self) -> Vec<u8> {
        let quoted = |pre: &str, arg: &[u8], post: &str| [pre.as_bytes(), arg, post.as_bytes()].concat();
        match self {
            Error::Usage => b"usage".to_vec(),
            Error::UnknownLongOption(arg) => quoted("unknown long option '", arg, "'."),
            Error::UnknownOption(c) => quoted("unknown option '-", &[*c], "'."),
            Error::NoParameter(c) => quoted("no parameter for '-", &[*c], "'."),
            Error::TooManyFolders => b"too many folders.".to_vec(),
            Error::SourceWithOperands => b"the 's' option can't co-exist with arguments.".to_vec(),
            Error::NoHomeDirectory => b"failed in getting home directory.".to_vec(),
            Error::HomeUnknown => b"can't change directory to home.".to_vec(),
            Error::ChangeDirectory(dir) => quoted("can't change directory to ", dir, "."),
            Error::ChangeFolder(folder) => quoted("can't change folder to ", folder, "."),
            Error::OpenFolder(folder) => quoted("can't open folder: ", folder, "."),
            Error::OpenFile(path) => quoted("can't open ", path, "."),
            Error::FolderNameTooLong => b"folder name is too long.".to_vec(),
            Error::DefaultFolderMissing => b"default folder is not exist.".to_vec(),
            Error::CommandNotFound(command) => quoted("'", command, "' command not found."),
            Error::Pattern(e) => e.message().as_bytes().to_vec(),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&String::from_utf8_lossy(&self.message()))
    }
}

impl std::error::Error for Error {}

impl From<PatternError> for Error {
    fn from(e: PatternError) -> Self {
        Error::Pattern(e)
    }
}

/// Prints diagnostics prefixed with the program name.
pub struct Reporter {
    progname: Vec<u8>,
}

impl Reporter {
    pub fn new(progname: Vec<u8>) -> Self {
        Reporter { progname }
    }

    pub fn progname(&self) -> &[u8] {
        &self.progname
    }

    /// Prints `progname: message` to stderr.
    pub fn warn(&self, e: &Error) {
        let line = [&self.progname[..], b": ", &e.message(), b"\n"].concat();
        let _ = io::stderr().write_all(&line);
    }
}
