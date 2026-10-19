//! Buffered standard output.

use std::io::{self, BufWriter, StdoutLock, Write};

/// Standard output, buffered until [`Output::flush`] like stdio of C.
///
/// Write errors are deliberately ignored as `printf()` in mewl does:
/// scanning goes on and the exit status is not affected. A closed
/// pipe terminates the process by SIGPIPE (see [`crate::sys`]).
pub struct Output(BufWriter<StdoutLock<'static>>);

impl Output {
    pub fn new() -> Self {
        Output(BufWriter::with_capacity(8192, io::stdout().lock()))
    }

    pub fn put(&mut self, bytes: &[u8]) -> &mut Self {
        let _ = self.0.write_all(bytes);
        self
    }

    pub fn flush(&mut self) {
        let _ = self.0.flush();
    }
}
