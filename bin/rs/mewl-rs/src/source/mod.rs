//! Where messages come from.
//!
//! - [`FolderScanner`]: message files in folders (default).
//! - [`ListReader`]: file names read from a file or stdin (`-i`),
//!   or from the output of an external command (`-e`).
//!
//! Both are iterators of [`Entry`]. An error ends the iteration in
//! effect since mewl exits with it.

mod folders;
mod list;

use std::ffi::OsStr;
use std::fs::{File, OpenOptions};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::OpenOptionsExt;

pub use folders::FolderScanner;
pub use list::ListReader;

use crate::error::{Error, Result};
use crate::folder::Navigator;

const MAX_FILE_NAME_LEN: usize = libc::PATH_MAX as usize;

pub struct Message {
    pub file: File,
    /// The message number without the suffix.
    pub name: Vec<u8>,
    pub folder: Vec<u8>,
}

pub enum Entry {
    /// The number of messages to come (`NumOfMsg:`).
    Count(usize),
    Message(Message),
}

/// What sources share.
#[derive(Clone, Debug)]
pub struct Settings {
    pub navigator: Navigator,
    /// The suffix of message files (`-x`).
    pub suffix: Vec<u8>,
    /// No folder is specified, so the folder is the default one.
    pub is_default: bool,
}

pub enum Source {
    Folders(FolderScanner),
    List(ListReader),
}

impl Iterator for Source {
    type Item = Result<Entry>;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Source::Folders(s) => s.next(),
            Source::List(s) => s.next(),
        }
    }
}

impl Source {
    /// See [`ListReader::command_failed`].
    pub fn command_failed(&mut self) -> bool {
        match self {
            Source::Folders(_) => false,
            Source::List(s) => s.command_failed(),
        }
    }
}

/// Whether a message is a regular file, checked unless `-n`. A file
/// whose type is unknown is not, as in mewl.
pub fn is_regular(file: &File) -> bool {
    file.metadata().is_ok_and(|m| m.file_type().is_file())
}

fn open(name: &[u8]) -> Option<File> {
    File::open(OsStr::from_bytes(name)).ok()
}

/// Opens a message without blocking, as mewl does, so that a FIFO
/// without a writer does not hang; it is not a regular file, and read
/// with `-n`, it is empty.
fn open_message(name: &[u8]) -> Option<File> {
    OpenOptions::new().read(true).custom_flags(libc::O_NONBLOCK).open(OsStr::from_bytes(name)).ok()
}

fn check_folder_name(folder: &[u8]) -> Result<()> {
    if folder.len() >= MAX_FILE_NAME_LEN {
        return Err(Error::FolderNameTooLong);
    }
    Ok(())
}
