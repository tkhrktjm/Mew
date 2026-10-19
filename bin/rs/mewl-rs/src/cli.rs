//! Command line options.
//!
//! The syntax is the one of `Getopt()` in utils.c: each option is a
//! separate argument (`-l 0`, not `-l0` nor `-al`), an argument of an
//! option is the next one even if it begins with `-`, and options end
//! at the first non-option argument or a lone `-`.

use std::io::{self, Write};

use crate::compat::atoi;
use crate::error::{Error, Result};
use crate::folder::{FolderRange, Navigator, parse_number};
use crate::header::FieldSet;
use crate::flavor::Flavor;
use crate::printer::Mode;
use crate::source::Settings;

const VERSION: &str = "version 6.11 20260414 Mew developing team";

const OPTSTRING: &[u8] = b"ab:c:d:e:f:hi:l:m:np:s:vwx:";

const MAX_FOLDER: usize = 16;

/* Some lines end with a space as mewl does. mewl lacks "<suffix>"
in the line of -x (see help()). */
const HELP: &str = " -a            Print all necessary fields when picking.
 -b  <mdir>    Mail home.
 -c  <bodylen> Max body length to dump.
 -d  <field>   A field to be extracted.
 -e  <command> An external command to fetch mailbox.
 -m  <options> The second argument for the external command.
 -f  <fields>  Fields to display.
 -h            Display this help message.
 -i  <file>    A file name to be read.
 -l  <length>  Field max length to dump. (0 means no limit)
 -n            Don't use fstat().
 -p  <pattern> Pick pattern.
 -s  <src>     Message source: '+folder range'.
 -w            Wait for stdio input to synchronize.
 -v            Display the version.
 -x  <suffix>  Use this suffix.

<range> = N | [start]-[end] | last:N
<pattern> = 'key=val' | 'key!=val' | '!<pattern>' | '(<pattern>)'\x20
          | '<pattern>&<pattern>'\x20
          | '<pattern>|<pattern>'\x20

Default is mdir = ~/Mail,
length = 5, folder = +inbox, range = 0, bodylen = 20
field = Subject:,Date:,From:,To:,Cc:,Content-Type:,Content-Transfer-Encoding:,X-Mew-UIDL:,Message-ID:,In-Reply-To:,References:,X-Mew-Ref:
";

pub enum Command {
    Help,
    Version,
    Run(Box<Options>),
}

pub struct Options {
    /// -a
    pub all_fields: bool,
    /// -b
    pub mail_home: Vec<u8>,
    /// -c
    pub body_len: i32,
    /// -d
    pub sort_key: Option<Vec<u8>>,
    /// -e
    pub exec_command: Option<Vec<u8>>,
    /// -m
    pub exec_args: Option<Vec<u8>>,
    /// -f
    pub fields: Option<Vec<u8>>,
    /// -i
    pub input_file: Option<Vec<u8>>,
    /// -l
    pub field_len: i32,
    /// cleared by -n
    pub use_fstat: bool,
    /// -p
    pub pattern: Option<Vec<u8>>,
    /// -s
    pub sources: Vec<FolderRange>,
    /// -w
    pub wait: bool,
    /// -x
    pub suffix: Vec<u8>,
    /// `[folder [range]]`
    pub operands: Vec<Vec<u8>>,
    /// How options are read and what is done.
    pub flavor: Flavor,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            all_fields: false,
            mail_home: b"Mail".to_vec(),
            body_len: 20,
            sort_key: None,
            exec_command: None,
            exec_args: None,
            fields: None,
            input_file: None,
            field_len: 5,
            use_fstat: true,
            pattern: None,
            sources: Vec::new(),
            wait: false,
            suffix: b".mew".to_vec(),
            operands: Vec::new(),
            flavor: Flavor::Compat,
        }
    }
}

impl Options {
    /// The folders to be scanned (`-s` or operands), and whether the
    /// folder is the default one, that is, no folder is specified.
    pub fn folder_ranges(&self) -> Result<(Vec<FolderRange>, bool)> {
        if !self.sources.is_empty() {
            if !self.operands.is_empty() {
                return Err(Error::SourceWithOperands);
            }
            return Ok((self.sources.clone(), false));
        }
        match self.operands.as_slice() {
            [] => Ok((vec![self.folder_range(None, None)?], true)),
            [folder] => Ok((vec![self.folder_range(Some(folder), None)?], false)),
            [folder, range] => Ok((vec![self.folder_range(Some(folder), Some(range))?], false)),
            _ => Err(Error::Usage),
        }
    }

    /// mewl reads a number in a range by `atoi()`, so that garbage is
    /// 0 and a negative number wraps around. `mewl-rs` refuses them.
    fn folder_range(&self, folder: Option<&[u8]>, range: Option<&[u8]>) -> Result<FolderRange> {
        if self.flavor.fixes_bugs() {
            FolderRange::parse_strict(folder, range)
        } else {
            Ok(FolderRange::parse(folder, range))
        }
    }

    /// The argument of `-c` or `-l`, likewise.
    fn count(&self, arg: &[u8]) -> Result<i32> {
        if !self.flavor.fixes_bugs() {
            return Ok(atoi(arg));
        }
        parse_number(arg)
            .and_then(|n| i32::try_from(n).ok())
            .ok_or_else(|| Error::InvalidNumber(arg.to_vec()))
    }

    /// What is printed: pick (`-p`), sort (`-d`), or scan.
    pub fn mode(&self) -> Result<Mode> {
        Ok(match (&self.pattern, &self.sort_key) {
            (Some(pattern), _) => Mode::Pick { pattern: self.flavor.parse_pattern(pattern)?, all_fields: self.all_fields },
            (None, Some(key)) => Mode::Sort { key: key.clone() },
            (None, None) => Mode::Scan,
        })
    }

    /// The fields that scan prints (`-f`).
    pub fn field_set(&self) -> FieldSet {
        FieldSet::parse(self.fields.as_deref().unwrap_or(FieldSet::DEFAULT))
    }

    pub fn settings(&self, is_default: bool) -> Settings {
        Settings {
            navigator: Navigator::new(self.mail_home.clone()),
            suffix: self.suffix.clone(),
            is_default,
            flavor: self.flavor,
        }
    }
}

pub fn parse(argv: &[Vec<u8>], flavor: Flavor) -> Result<Command> {
    let mut opts = Options { flavor, ..Options::default() };
    let mut args = argv.iter().skip(1).peekable();

    while let Some(arg) = args.next_if(|a| a.len() >= 2 && a[0] == b'-') {
        let (c, rest) = (arg[1], &arg[2..]);
        if !rest.is_empty() {
            return Err(Error::UnknownLongOption(arg.clone()));
        }
        /* mewl takes ':' in OPTSTRING for an option, and then shows
        the usage. */
        let pos = OPTSTRING.iter().position(|&o| o == c);
        let Some(pos) = pos.filter(|_| !(c == b':' && flavor.fixes_bugs())) else {
            return Err(Error::UnknownOption(c));
        };
        let optarg = if OPTSTRING.get(pos + 1) == Some(&b':') {
            match args.next() {
                Some(a) => a.clone(),
                None => return Err(Error::NoParameter(c)),
            }
        } else {
            Vec::new()
        };

        match c {
            b'a' => opts.all_fields = true,
            b'b' => opts.mail_home = optarg,
            b'c' => opts.body_len = opts.count(&optarg)?,
            b'd' => opts.sort_key = Some(optarg),
            b'e' => opts.exec_command = Some(optarg),
            b'f' => opts.fields = Some(optarg),
            b'h' => return Ok(Command::Help),
            b'i' => opts.input_file = Some(optarg),
            b'l' => opts.field_len = opts.count(&optarg)?,
            b'm' => opts.exec_args = Some(optarg),
            b'n' => opts.use_fstat = false,
            b'p' => opts.pattern = Some(optarg),
            b's' => {
                if opts.sources.len() >= MAX_FOLDER {
                    return Err(Error::TooManyFolders);
                }
                let range = opts.folder_range(Some(&optarg), None)?;
                opts.sources.push(range);
            }
            b'v' => return Ok(Command::Version),
            b'w' => opts.wait = true,
            b'x' => opts.suffix = optarg,
            /* ':' in OPTSTRING */
            _ => return Err(Error::Usage),
        }
    }

    opts.operands = args.cloned().collect();
    Ok(Command::Run(Box::new(opts)))
}

/// `getprognm()`: the basename in lower case.
pub fn program_name(argv0: &[u8]) -> Vec<u8> {
    let base = argv0.rsplit(|&c| c == b'/').next().unwrap_or(argv0);
    base.to_ascii_lowercase()
}

pub fn usage(progname: &[u8]) {
    let line = [b"Usage: ", progname, b" [options] [folder [range]]\n"].concat();
    let _ = io::stderr().write_all(&line);
}

pub fn help(progname: &[u8], flavor: Flavor) {
    let head = [b"Help: ", progname, b"\n\n Message scanner.\n\n"].concat();
    let _ = io::stderr().write_all(&head);
    usage(progname);
    let help = match flavor.fixes_bugs() {
        true => HELP.to_string(),
        false => HELP.replace(" -x  <suffix>  ", " -x            "),
    };
    let _ = io::stderr().write_all(help.as_bytes());
}

pub fn version(progname: &[u8]) {
    let line = [progname, b" ", VERSION.as_bytes(), b"\n"].concat();
    let _ = io::stderr().write_all(&line);
}
