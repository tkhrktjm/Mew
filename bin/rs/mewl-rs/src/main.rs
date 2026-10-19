//! mewl-rs -- Message scanner for Mew 4 or later
//!
//! A Rust port of mewl. It extracts fields from messages in folders
//! for Mew. The input, the output, and the options are the same as
//! mewl's; see mewl(1).
//!
//! ```text
//! mewl-rs [options] [folder [range]]
//! ```
//!
//! As `mewl-rs`, it decodes MIME-encoded header values to UTF-8 more
//! than mewl does. Invoked by any other name, e.g. `mewl-rs-compat`
//! or `mewl` through a link, it is the same as mewl (see [`Flavor`]).

mod bytes;
mod cli;
mod compat;
mod error;
mod flavor;
mod folder;
mod header;
mod mime;
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
use error::{Error, Reporter, Result};
use flavor::Flavor;
use output::Output;
use printer::{Printed, Printer};
use source::{Entry, FolderScanner, ListReader, RegularFileCheck, Source};

fn main() -> ExitCode {
    sys::restore_sigpipe();

    let argv: Vec<Vec<u8>> = env::args_os().map(OsStringExt::into_vec).collect();
    let progname = cli::program_name(argv.first().map_or(&[][..], Vec::as_slice));
    let flavor = Flavor::from_program_name(&progname);
    let reporter = Reporter::new(progname);

    let mut out = Output::new();
    let result = cli::parse(&argv).and_then(|command| match command {
        Command::Help => {
            cli::help(reporter.progname());
            Ok(())
        }
        Command::Version => {
            cli::version(reporter.progname());
            Ok(())
        }
        Command::Run(opts) => run(&opts, flavor, &reporter, &mut out),
    });
    out.flush();

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(Error::Usage) => {
            cli::usage(reporter.progname());
            ExitCode::FAILURE
        }
        Err(e) => {
            reporter.warn(&e);
            ExitCode::FAILURE
        }
    }
}

fn run(opts: &Options, flavor: Flavor, reporter: &Reporter, out: &mut Output) -> Result<()> {
    let (ranges, is_default) = opts.folder_ranges()?;
    let fields = opts.field_set();
    let mode = opts.mode(flavor)?;
    let count = opts.fields.is_some() && !matches!(mode, printer::Mode::Pick { .. });

    /* With -w, a byte from stdin is awaited after each record. This
    is read from a duplicate since stdin may be used for -i -. */
    let mut sync = opts
        .wait
        .then(|| io::stdin().as_fd().try_clone_to_owned().ok())
        .flatten()
        .map(|fd| BufReader::new(File::from(fd)));

    let settings = opts.settings(is_default);
    let folder = &ranges[0].folder;
    let source = if let Some(path) = &opts.input_file {
        Source::List(ListReader::open(path, folder, settings)?)
    } else if let Some(command) = &opts.exec_command {
        Source::List(ListReader::spawn(command, opts.exec_args.as_deref(), folder, settings, reporter)?)
    } else {
        Source::Folders(FolderScanner::new(ranges, settings, count))
    };

    let mut printer = Printer::new(mode, fields, opts.body_len, opts.field_len, flavor);
    let mut regular = RegularFileCheck::default();
    for entry in source {
        match entry? {
            Entry::Count(n) => out.put(format!("NumOfMsg: {n}\n").as_bytes()).flush(),
            Entry::Message(mut msg) => {
                if opts.use_fstat && !regular.is_regular(&msg.file) {
                    continue;
                }
                if printer.print(&mut msg, out)? == Printed::Record
                    && let Some(sync) = &mut sync
                {
                    out.flush();
                    let _ = sync.read(&mut [0]);
                }
            }
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
