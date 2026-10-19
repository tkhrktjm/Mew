//! `mewl-rs` and `mewl-rs-compat`.

use std::borrow::Cow;

use crate::bytes::{BytesExt, Case};
use crate::compat::GLIBC;
use crate::mime;
use crate::pattern::{Pattern, PatternError};

/// What the program does, which depends on the name it is invoked as.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flavor {
    /// `mewl-rs-compat` (and any name but `mewl-rs`): the same as mewl,
    /// which decodes encoded-words in a field body searched by pick.
    Compat,
    /// `mewl-rs`: in addition, the value printed by sort (`-d`)
    /// and the whole header searched by pick (`head`) are decoded,
    /// encoded-words are found anywhere and a character split between
    /// them is restored, raw ISO-2022-JP is converted, and a value in
    /// UTF-8 matches ignoring case on glibc, too. Some bugs of mewl
    /// are fixed: the last line of a message is kept, and a pattern is
    /// parsed strictly.
    Utf8,
}

impl Flavor {
    pub fn from_program_name(name: &[u8]) -> Self {
        /* Any other name, e.g. "mewl" as a drop-in replacement, is
        the same as mewl. */
        if name == b"mewl-rs" { Flavor::Utf8 } else { Flavor::Compat }
    }

    /// Parses a pick pattern. `mewl-rs` refuses a broken pattern, and
    /// takes words not quoted after a value as a part of it.
    pub(crate) fn parse_pattern(self, pattern: &[u8]) -> Result<Pattern, PatternError> {
        match self {
            Flavor::Compat => Pattern::parse(pattern),
            Flavor::Utf8 => Pattern::parse_strict(pattern),
        }
    }

    /// Whether the last line of a message is kept as it is. mewl
    /// overwrites the last byte with LF if it is not, and does not
    /// escape the last line of the body beginning with `.`.
    pub(crate) fn keeps_last_line(self) -> bool {
        self == Flavor::Utf8
    }

    /// How `key=val` compares ignoring case.
    pub(crate) fn ignore_case(self) -> Case {
        match self {
            Flavor::Compat if GLIBC => Case::IgnoreAsGlibc,
            Flavor::Compat | Flavor::Utf8 => Case::Ignore,
        }
    }

    /// The text that pick compares for a field. `body` follows the key,
    /// so it begins with `:` if the key lacks it.
    pub(crate) fn field_body(self, body: &[u8]) -> Cow<'_, [u8]> {
        let after_colon = || body.strip_prefix(b":").unwrap_or(body);
        match self {
            Flavor::Compat if body.contains_bytes(b"=?", Case::Sensitive) => mime::decode_field(after_colon()).into(),
            Flavor::Compat => body.into(),
            Flavor::Utf8 => mime::decode_text(after_colon()).into(),
        }
    }

    /// The text that pick compares for `head`.
    pub(crate) fn whole_header(self, header: &[u8]) -> Cow<'_, [u8]> {
        match self {
            Flavor::Compat => header.into(),
            Flavor::Utf8 => mime::decode_header(header).into(),
        }
    }

    /// The value that sort prints.
    pub(crate) fn sort_value(self, value: &[u8]) -> Cow<'_, [u8]> {
        match self {
            Flavor::Compat => value.into(),
            Flavor::Utf8 => mime::decode_text(value).into(),
        }
    }
}
