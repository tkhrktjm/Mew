//! Message files in folders.

use std::fs;
use std::os::unix::ffi::OsStrExt;
use std::vec;

use super::{Entry, Message, Settings, open};
use crate::compat::atoi;
use crate::error::{Error, Result};
use crate::folder::{FolderRange, parse_number};

pub struct FolderScanner {
    ranges: vec::IntoIter<FolderRange>,
    settings: Settings,
    /// Yield [`Entry::Count`] before the messages of a folder.
    count: bool,
    /// The current folder and the message numbers left in it.
    folder: Vec<u8>,
    numbers: vec::IntoIter<u32>,
}

impl FolderScanner {
    /// With `count`, the number of messages is told if there is only
    /// one folder.
    pub fn new(ranges: Vec<FolderRange>, settings: Settings, count: bool) -> Self {
        FolderScanner {
            count: count && ranges.len() == 1,
            ranges: ranges.into_iter(),
            settings,
            folder: Vec::new(),
            numbers: Vec::new().into_iter(),
        }
    }

    /// The next message in the current folder.
    fn next_message(&mut self) -> Option<Message> {
        let suffix = &self.settings.suffix;
        self.numbers.by_ref().find_map(|num| {
            /* "%d" of unsigned int */
            let num = num as i32;
            [format!("{num}"), format!("0{num}")].into_iter().find_map(|name| {
                let name = name.into_bytes();
                let file = open(&[&name[..], suffix].concat()).or_else(|| open(&name))?;
                Some(Message { file, name, folder: self.folder.clone() })
            })
        })
    }

    /// Changes to the folder and lists its messages. Returns how many.
    fn scan_folder(&mut self, range: FolderRange) -> Result<usize> {
        /* A missing default folder leaves us in the Mail home. */
        self.settings.navigator.enter(&range.folder, self.settings.is_default)?;

        let entries = fs::read_dir(".").map_err(|_| Error::OpenFolder(range.folder.clone()))?;
        let mut numbers: Vec<u32> = entries
            .map_while(|entry| entry.ok())
            .filter_map(|entry| {
                let name = entry.file_name();
                let name = name.as_bytes();
                let num = self.message_number(name)?;
                /* mewl reads the number again by atoi(), which may
                differ from num if it overflows. */
                let num_read = if self.settings.flavor.fixes_bugs() { num } else { atoi(name) as u32 };
                range.contains(num).then_some(num_read)
            })
            .collect();
        numbers.sort_unstable();

        let count = range.count as usize;
        if count > 0 && count < numbers.len() {
            numbers.drain(..numbers.len() - count);
        }

        let n = numbers.len();
        self.folder = range.folder;
        self.numbers = numbers.into_iter();
        Ok(n)
    }

    /// The number of a message file: digits optionally followed by
    /// the suffix. In mewl, it overflows like int in C; in `mewl-rs`, a
    /// file with a number too large is not a message.
    fn message_number(&self, name: &[u8]) -> Option<u32> {
        let digits = name.iter().take_while(|c| c.is_ascii_digit()).count();
        let rest = &name[digits..];
        if !(rest.is_empty() || rest == self.settings.suffix) {
            return None;
        }
        let num = if self.settings.flavor.fixes_bugs() {
            parse_number(&name[..digits])?
        } else {
            name[..digits]
                .iter()
                .fold(0i32, |n, &c| n.wrapping_mul(10).wrapping_add(i32::from(c - b'0'))) as u32
        };
        (num != 0).then_some(num)
    }
}

impl Iterator for FolderScanner {
    type Item = Result<Entry>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(msg) = self.next_message() {
                return Some(Ok(Entry::Message(msg)));
            }
            let range = self.ranges.next()?;
            match self.scan_folder(range) {
                Ok(n) if self.count && n > 0 => return Some(Ok(Entry::Count(n))),
                Ok(_) => {}
                Err(e) => return Some(Err(e)),
            }
        }
    }
}
