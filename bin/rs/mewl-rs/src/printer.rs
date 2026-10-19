//! Output formats.
//!
//! - scan: `Folder:`, `Filename:`, the fields, an empty line, the
//!   first lines of the body, and `.` ending the record.
//! - sort (`-d key`): `name:value` of the field.
//! - pick (`-p pattern`): the name of a matched message, or its
//!   record of scan with `-a`.
//!
//! With [`Flavor::Utf8`], the value of sort is decoded to UTF-8. The
//! scan record is the same as mewl's since Mew decodes it.

use crate::error::Result;
use crate::flavor::Flavor;
use crate::header::{FieldSet, Header};
use crate::output::Output;
use crate::pattern::Pattern;
use crate::source::Message;

pub enum Mode {
    Scan,
    Sort { key: Vec<u8> },
    Pick { pattern: Pattern, all_fields: bool },
}

/// What [`Printer::print`] printed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Printed {
    Nothing,
    Line,
    /// A scan record, after which `-w` waits for the reader.
    Record,
}

pub struct Printer {
    mode: Mode,
    fields: FieldSet,
    body_len: i32,
    header: Header,
    flavor: Flavor,
}

impl Printer {
    pub fn new(mode: Mode, fields: FieldSet, body_len: i32, field_len: i32, flavor: Flavor) -> Self {
        Printer { mode, fields, body_len, header: Header::new(field_len, flavor), flavor }
    }

    pub fn print(&mut self, msg: &mut Message, out: &mut Output) -> Result<Printed> {
        self.header.load(&mut msg.file);
        match &self.mode {
            Mode::Scan => {
                self.print_record(msg, out);
                Ok(Printed::Record)
            }
            Mode::Sort { key } => {
                out.put(&msg.name);
                match self.header.field_value(key) {
                    Some(value) => out.put(b":").put(&self.flavor.sort_value(self.header.bytes(value))).put(b"\n"),
                    None => out.put(b": \n"),
                };
                Ok(Printed::Line)
            }
            Mode::Pick { pattern, all_fields } => {
                let header = &mut self.header;
                if !pattern.matches(&mut |key, value, case| header.search(key, value, case))? {
                    Ok(Printed::Nothing)
                } else if *all_fields {
                    self.print_record(msg, out);
                    Ok(Printed::Record)
                } else {
                    out.put(&msg.name).put(b"\n");
                    Ok(Printed::Line)
                }
            }
        }
    }

    fn print_record(&mut self, msg: &Message, out: &mut Output) {
        let summary = self.header.summarize(&self.fields, self.body_len);
        let header = &self.header;

        out.put(b"Folder: ").put(&msg.folder).put(b"\n");
        out.put(b"Filename: ").put(&msg.name).put(b"\n");
        for (i, range) in summary.fields.into_iter().enumerate() {
            let Some(range) = range else { continue };
            if self.fields.is_references(i) {
                out.put(FieldSet::REFERENCES);
            }
            out.put(header.bytes(range));
        }
        out.put(b"\n");
        for line in summary.body {
            if line.dot_stuffed {
                out.put(b".");
            }
            out.put(header.bytes(line.range));
        }
        out.put(b".\n");
    }
}
