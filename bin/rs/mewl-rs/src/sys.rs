//! The only place which needs `unsafe`.

use std::ffi::CStr;

/// Restores the default action of SIGPIPE, which Rust ignores.
///
/// mewl dies silently when its reader (Emacs) goes away.
pub fn restore_sigpipe() {
    // SAFETY: setting the default disposition of a signal has no
    // memory safety requirements.
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
}

/// The home directory in the password database, used when `HOME` is
/// unset or empty. The outer `None` means that the user is unknown.
pub fn home_from_passwd() -> Option<Option<Vec<u8>>> {
    // SAFETY: getpwuid() returns NULL or a pointer to a static entry
    // which is valid until the next call. It is copied immediately,
    // and this program is single-threaded.
    unsafe {
        let pw = libc::getpwuid(libc::getuid());
        if pw.is_null() {
            return None;
        }
        let dir = (*pw).pw_dir;
        Some((!dir.is_null()).then(|| CStr::from_ptr(dir).to_bytes().to_vec()))
    }
}
