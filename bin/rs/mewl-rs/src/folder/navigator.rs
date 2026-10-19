//! Changing the current directory to folders.
//!
//! mewl changes the current directory to a folder and opens messages
//! there, so a relative folder is relative to the previous one. The
//! same is done here.

use std::env;
use std::ffi::OsStr;
use std::os::unix::ffi::{OsStrExt, OsStringExt};

use crate::error::{Error, Result};
use crate::sys;

/// The result of [`Navigator::enter`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Entered {
    Folder,
    /// The default folder does not exist. The current directory is
    /// the Mail home.
    MissingDefault,
}

/// Changes to folders in the Mail home.
#[derive(Clone, Debug)]
pub struct Navigator {
    /// `~/dir`, `/dir`, or `dir` relative to `~`.
    mail_home: Vec<u8>,
}

impl Navigator {
    pub fn new(mail_home: Vec<u8>) -> Self {
        Navigator { mail_home }
    }

    /// Changes to a folder: `+folder` in the Mail home, `~/dir`, or
    /// `dir`. If `is_default`, a missing `+folder` is not an error.
    pub fn enter(&self, folder: &[u8], is_default: bool) -> Result<Entered> {
        let error = || Error::ChangeFolder(folder.to_vec());
        let ok = match folder {
            [b'+', rest @ ..] => {
                self.enter_mail_home()?;
                if rest.is_empty() || chdir(rest) {
                    true
                } else if is_default {
                    return Ok(Entered::MissingDefault);
                } else {
                    false
                }
            }
            [b'~', rest @ ..] => change_under_home(rest, error)?,
            _ => chdir(folder),
        };
        if ok { Ok(Entered::Folder) } else { Err(error()) }
    }

    fn enter_mail_home(&self) -> Result<()> {
        let dir = &self.mail_home;
        let error = || Error::ChangeDirectory(dir.clone());
        let ok = match dir.as_slice() {
            [b'~', rest @ ..] => change_under_home(rest, error)?,
            [b'/', ..] => chdir(dir),
            _ => {
                change_to_home()?;
                chdir(dir)
            }
        };
        if ok { Ok(()) } else { Err(error()) }
    }
}

fn chdir(dir: &[u8]) -> bool {
    env::set_current_dir(OsStr::from_bytes(dir)).is_ok()
}

fn change_to_home() -> Result<()> {
    let home = match env::var_os("HOME").map(OsStringExt::into_vec) {
        Some(home) if !home.is_empty() => home,
        _ => match sys::home_from_passwd() {
            None => return Err(Error::NoHomeDirectory),
            Some(None) => return Err(Error::HomeUnknown),
            Some(Some(home)) => home,
        },
    };
    if chdir(&home) { Ok(()) } else { Err(Error::ChangeDirectory(home)) }
}

/// Changes to `~` or `~/path`, given what follows `~`. `Ok(false)`
/// means that `~/path` does not exist; `error` is for `~path`.
fn change_under_home(path: &[u8], error: impl Fn() -> Error) -> Result<bool> {
    change_to_home()?;
    match path {
        [] => Ok(true),
        [b'/', rest @ ..] => Ok(rest.is_empty() || chdir(rest)),
        _ => Err(error()),
    }
}
