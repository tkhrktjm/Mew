//! `mewl-rs` and `mewl-rs-utf8`.

use std::borrow::Cow;

use crate::bytes::{BytesExt, Case};
use crate::compat::GLIBC;
use crate::mime;

/// What the program does, which depends on the name it is invoked as.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flavor {
    /// `mewl-rs` (and any other name): the same as mewl, which decodes
    /// encoded-words in a field body searched by pick.
    Mewl,
    /// `mewl-rs-utf8`: in addition, the value printed by sort (`-d`)
    /// and the whole header searched by pick (`head`) are decoded,
    /// encoded-words are found anywhere and a character split between
    /// them is restored, raw ISO-2022-JP is converted, and a value in
    /// UTF-8 matches ignoring case on glibc, too.
    Utf8,
}

impl Flavor {
    pub fn from_program_name(name: &[u8]) -> Self {
        if name == b"mewl-rs-utf8" { Flavor::Utf8 } else { Flavor::Mewl }
    }

    /// How `key=val` compares ignoring case.
    pub(crate) fn ignore_case(self) -> Case {
        match self {
            Flavor::Mewl if GLIBC => Case::IgnoreAsGlibc,
            Flavor::Mewl | Flavor::Utf8 => Case::Ignore,
        }
    }

    /// The text that pick compares for a field. `body` follows the key,
    /// so it begins with `:` if the key lacks it.
    pub(crate) fn field_body(self, body: &[u8]) -> Cow<'_, [u8]> {
        let after_colon = || body.strip_prefix(b":").unwrap_or(body);
        match self {
            Flavor::Mewl if body.contains_bytes(b"=?", Case::Sensitive) => mime::decode_field(after_colon()).into(),
            Flavor::Mewl => body.into(),
            Flavor::Utf8 => mime::decode_text(after_colon()).into(),
        }
    }

    /// The text that pick compares for `head`.
    pub(crate) fn whole_header(self, header: &[u8]) -> Cow<'_, [u8]> {
        match self {
            Flavor::Mewl => header.into(),
            Flavor::Utf8 => mime::decode_header(header).into(),
        }
    }

    /// The value that sort prints.
    pub(crate) fn sort_value(self, value: &[u8]) -> Cow<'_, [u8]> {
        match self {
            Flavor::Mewl => value.into(),
            Flavor::Utf8 => mime::decode_text(value).into(),
        }
    }
}
