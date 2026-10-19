//! Errors and diagnostics.
//!
//! mewl reports a fatal error as `progname: message` on stderr and
//! exits with 1 (`warn_exit()`). Here the error is propagated up to
//! `main()` as [`Error`] instead.

use std::io::{self, Write};
use std::sync::OnceLock;

#[derive(Debug)]
pub enum Error {
    /// Reported as `progname: message`.
    Fatal(Vec<u8>),
    /// Reported with the usage line.
    Usage,
}

pub type Result<T> = std::result::Result<T, Error>;

/// Builds [`Error::Fatal`] by concatenating byte-like parts.
///
/// File and folder names are not necessarily UTF-8, so the message
/// is made of bytes.
macro_rules! fatal {
    ($($part:expr),+ $(,)?) => {
        $crate::error::Error::Fatal([$(AsRef::<[u8]>::as_ref(&$part)),+].concat())
    };
}
pub(crate) use fatal;

static PROGRAM_NAME: OnceLock<Vec<u8>> = OnceLock::new();

pub fn set_program_name(name: &[u8]) {
    let _ = PROGRAM_NAME.set(name.to_vec());
}

pub fn program_name() -> &'static [u8] {
    PROGRAM_NAME.get().map_or(b"", Vec::as_slice)
}

/// Prints `progname: message` to stderr.
pub fn warn(msg: &[u8]) {
    let line = [program_name(), b": ", msg, b"\n"].concat();
    let _ = io::stderr().write_all(&line);
}
