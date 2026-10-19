//! `mewl-rs-utf8`.

use std::process::ExitCode;

use mewl_rs::Flavor;

fn main() -> ExitCode {
    mewl_rs::main(Some(Flavor::Utf8))
}
