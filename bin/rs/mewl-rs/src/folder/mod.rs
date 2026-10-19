//! Folders: their names with ranges, and changing directories to them.

mod navigator;
mod range;

pub use navigator::{Entered, Navigator};
pub use range::FolderRange;
