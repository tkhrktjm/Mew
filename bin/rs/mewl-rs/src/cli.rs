//! Command line options.
//!
//! The syntax is the one of `Getopt()` in utils.c: each option is a
//! separate argument (`-l 0`, not `-l0` nor `-al`), an argument of an
//! option is the next one even if it begins with `-`, and options end
//! at the first non-option argument or a lone `-`.

use std::io::{self, Write};

use crate::compat::atoi;
use crate::error::{Error, Result, fatal};
use crate::folder::FolderRange;

const VERSION: &str = "version 6.11 20260414 Mew developing team";

const OPTSTRING: &[u8] = b"ab:c:d:e:f:hi:l:m:np:s:vwx:";

const MAX_FOLDER: usize = 16;

/* Some lines end with a space as mewl does. */
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
 -x            Use this suffix.

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
        }
    }
}

impl Options {
    /// The folders to be scanned, and whether the folder is the
    /// default one, that is, no folder is specified.
    pub fn folder_ranges(&self) -> Result<(Vec<FolderRange>, bool)> {
        if !self.sources.is_empty() {
            if !self.operands.is_empty() {
                return Err(fatal!("the 's' option can't co-exist with arguments."));
            }
            return Ok((self.sources.clone(), false));
        }
        match self.operands.as_slice() {
            [] => Ok((vec![FolderRange::parse(None, None)], true)),
            [folder] => Ok((vec![FolderRange::parse(Some(folder), None)], false)),
            [folder, range] => Ok((vec![FolderRange::parse(Some(folder), Some(range))], false)),
            _ => Err(Error::Usage),
        }
    }
}

pub fn parse(argv: &[Vec<u8>]) -> Result<Command> {
    let mut opts = Options::default();
    let mut args = argv.iter().skip(1).peekable();

    while let Some(arg) = args.next_if(|a| a.len() >= 2 && a[0] == b'-') {
        let (c, rest) = (arg[1], &arg[2..]);
        if !rest.is_empty() {
            return Err(fatal!("unknown long option '", arg, "'."));
        }
        let Some(pos) = OPTSTRING.iter().position(|&o| o == c) else {
            return Err(fatal!("unknown option '-", [c], "'."));
        };
        let optarg = if OPTSTRING.get(pos + 1) == Some(&b':') {
            match args.next() {
                Some(a) => a.clone(),
                None => return Err(fatal!("no parameter for '-", [c], "'.")),
            }
        } else {
            Vec::new()
        };

        match c {
            b'a' => opts.all_fields = true,
            b'b' => opts.mail_home = optarg,
            b'c' => opts.body_len = atoi(&optarg),
            b'd' => opts.sort_key = Some(optarg),
            b'e' => opts.exec_command = Some(optarg),
            b'f' => opts.fields = Some(optarg),
            b'h' => return Ok(Command::Help),
            b'i' => opts.input_file = Some(optarg),
            b'l' => opts.field_len = atoi(&optarg),
            b'm' => opts.exec_args = Some(optarg),
            b'n' => opts.use_fstat = false,
            b'p' => opts.pattern = Some(optarg),
            b's' => {
                if opts.sources.len() >= MAX_FOLDER {
                    return Err(fatal!("too many folders."));
                }
                opts.sources.push(FolderRange::parse(Some(&optarg), None));
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

pub fn help(progname: &[u8]) {
    let head = [b"Help: ", progname, b"\n\n Message scanner.\n\n"].concat();
    let _ = io::stderr().write_all(&head);
    usage(progname);
    let _ = io::stderr().write_all(HELP.as_bytes());
}

pub fn version(progname: &[u8]) {
    let line = [progname, b" ", VERSION.as_bytes(), b"\n"].concat();
    let _ = io::stderr().write_all(&line);
}
