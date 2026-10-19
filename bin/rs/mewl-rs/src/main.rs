//! mewl-rs -- Message scanner for Mew 4 or later
//!
//! A Rust port of mewl. It extracts fields from messages in folders
//! for Mew. The input, the output, and the options are the same as
//! mewl's; see mewl(1).
//!
//! ```text
//! mewl-rs [options] [folder [range]]
//! ```

mod cli;
mod compat;
mod error;
mod folder;
mod header;
mod output;
mod pattern;
mod printer;
mod source;
mod sys;

use std::env;
use std::fs::File;
use std::io::{self, BufReader, Read};
use std::os::fd::AsFd;
use std::os::unix::ffi::OsStringExt;
use std::process::ExitCode;

use cli::{Command, Options};
use error::{Error, Result};
use header::FieldSet;
use output::Output;
use pattern::Pattern;
use printer::{Mode, Printer};
use source::{FolderScanner, ListReader, Source};

fn main() -> ExitCode {
    sys::restore_sigpipe();

    let argv: Vec<Vec<u8>> = env::args_os().map(OsStringExt::into_vec).collect();
    let progname = cli::program_name(argv.first().map_or(&[][..], Vec::as_slice));
    error::set_program_name(&progname);

    let mut out = Output::new();
    let result = cli::parse(&argv).and_then(|command| match command {
        Command::Help => {
            cli::help(&progname);
            Ok(())
        }
        Command::Version => {
            cli::version(&progname);
            Ok(())
        }
        Command::Run(opts) => run(*opts, &mut out),
    });
    out.flush();

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(Error::Usage) => {
            cli::usage(&progname);
            ExitCode::FAILURE
        }
        Err(Error::Fatal(msg)) => {
            error::warn(&msg);
            ExitCode::FAILURE
        }
    }
}

fn run(opts: Options, out: &mut Output) -> Result<()> {
    let (ranges, is_default) = opts.folder_ranges()?;
    let fields = FieldSet::parse(opts.fields.as_deref().unwrap_or(FieldSet::DEFAULT));

    let mode = match (&opts.pattern, &opts.sort_key) {
        (Some(pattern), _) => Mode::Pick { pattern: Pattern::parse(pattern)?, all_fields: opts.all_fields },
        (None, Some(key)) => Mode::Sort { key: key.clone() },
        (None, None) => Mode::Scan,
    };
    let print_count = opts.fields.is_some() && !matches!(mode, Mode::Pick { .. });

    /* With -w, a byte from stdin is awaited after each record. This
    is read from a duplicate since stdin may be used for -i -. */
    let mut sync = if opts.wait {
        io::stdin().as_fd().try_clone_to_owned().ok().map(|fd| BufReader::new(File::from(fd)))
    } else {
        None
    };

    let folder = &ranges[0].folder;
    let (home, suffix) = (opts.mail_home.clone(), opts.suffix.clone());
    let mut source = if let Some(path) = &opts.input_file {
        Source::List(ListReader::open(path, folder, home, suffix, is_default)?)
    } else if let Some(command) = &opts.exec_command {
        let args = opts.exec_args.as_deref();
        Source::List(ListReader::spawn(command, args, folder, home, suffix, is_default)?)
    } else {
        Source::Folders(FolderScanner::new(ranges, home, suffix, is_default, print_count))
    };

    let mut printer = Printer::new(mode, fields, opts.body_len, opts.field_len);
    let mut mode = 0;
    while let Some(mut msg) = source.next_message(out)? {
        if opts.use_fstat && !msg.is_regular(&mut mode) {
            continue;
        }
        let printed_record = printer.print(&mut msg, out)?;
        if printed_record && let Some(sync) = &mut sync {
            out.flush();
            let _ = sync.read(&mut [0]);
        }
    }
    Ok(())
}

/*
 * Copyright (C) 2000-2005 Mew developing team.
 * All rights reserved.
 *
 * Redistribution and use in source and binary forms, with or without
 * modification, are permitted provided that the following conditions
 * are met:
 *
 * 1. Redistributions of source code must retain the above copyright
 *    notice, this list of conditions and the following disclaimer.
 * 2. Redistributions in binary form must reproduce the above copyright
 *    notice, this list of conditions and the following disclaimer in the
 *    documentation and/or other materials provided with the distribution.
 * 3. Neither the name of the team nor the names of its contributors
 *    may be used to endorse or promote products derived from this software
 *    without specific prior written permission.
 *
 * THIS SOFTWARE IS PROVIDED BY THE TEAM AND CONTRIBUTORS ``AS IS'' AND
 * ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
 * IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR
 * PURPOSE ARE DISCLAIMED.  IN NO EVENT SHALL THE TEAM OR CONTRIBUTORS BE
 * LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR
 * CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF
 * SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR
 * BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
 * WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE
 * OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN
 * IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
 */
