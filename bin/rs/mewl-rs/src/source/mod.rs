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
use std::fs::File;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::MetadataExt;

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

/// Tells whether a message is a regular file (unless `-n`).
#[derive(Default)]
pub struct RegularFileCheck {
    /// mewl keeps the previous mode if `fstat()` fails.
    mode: u32,
}

impl RegularFileCheck {
    /// The `S_IFREG` bit of `st_mode`.
    const S_IFREG: u32 = 0o100000;

    /// mewl tests the `S_IFREG` bit, not the file type, so a socket
    /// passes, for instance.
    pub fn is_regular(&mut self, file: &File) -> bool {
        if let Ok(meta) = file.metadata() {
            self.mode = meta.mode();
        }
        self.mode & Self::S_IFREG != 0
    }
}

fn open(name: &[u8]) -> Option<File> {
    File::open(OsStr::from_bytes(name)).ok()
}

fn check_folder_name(folder: &[u8]) -> Result<()> {
    if folder.len() >= MAX_FILE_NAME_LEN {
        return Err(Error::FolderNameTooLong);
    }
    Ok(())
}
