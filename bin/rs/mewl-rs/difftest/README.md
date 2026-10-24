# difftest

A differential test of mewl-rs against the C mewl, or of one mewl-rs
against another. It runs two binaries with the same arguments and
input over a corpus of messages, and compares the exit status, stdout,
and stderr of each run. The counts in the commit messages of mewl-rs
come from it.
The version printed by `-v` is replaced before comparing, as mewl-rs
has a version of its own.

A Japanese translation is in [README.ja.md](README.ja.md).

Only Python 3 is needed besides the binaries.

## Files

- `difftest.py`: the test (`difftest.py --help`).
- `mkcorpus.py`: makes the corpus; `difftest.py` runs it into
  `work/` when the corpus is not there yet or is older than it.
- `classify.py`: sorts the mismatches dumped by `difftest.py --dump`
  by the change that should explain them.
- `fake-inc`: a stand-in for incm, for `mewl -e`.

## Building the binaries

The C mewl of a revision, with iconv and, if installed, utf8proc
(libutf8proc-dev on Debian; the mewl of main uses it when found):

```
% mkdir /tmp/c && git archive 464013a | tar -x -C /tmp/c
% cd /tmp/c/bin && ./configure && make mewl
```

mewl-rs:

```
% cd bin/rs/mewl-rs && cargo build --release
```

## Running

```
% cd bin/rs/mewl-rs/difftest
% ./difftest.py /tmp/c/bin/mewl ../target/release/mewl-rs --name-b mewl-rs-compat
```

mewl-rs acts by the name it is invoked as, so give the name with
`--name-a` or `--name-b`: `mewl-rs` decodes more than mewl, and any
other name, such as `mewl` (the default) or `mewl-rs-compat`, is the
same as mewl.

`--suite` chooses the cases (all but `wellformed` by default):

| suite | cases | what |
|---|---:|---|
| `base` | 3996 | options, folders and ranges, scan, sort (`-d`), `-i`, `-e`, `-w`, errors, and 3000 fuzzed pick patterns |
| `mime` | 1227 | pick and sort on encoded-words of RFC 2047 |
| `uni` | 1626 | pick with Unicode case folding and NFC |
| `utf7` | 471 | pick and sort on encoded-words in UTF-7 and UTF-7-IMAP, valid and invalid |
| `wellformed` | 3000 | well-formed pick patterns on an empty folder |

`--corpus-a` and `--corpus-b` choose a variant of the corpus for each
binary, to tell the effects of fixes apart:

| variant | change |
|---|---|
| `orig` | none |
| `lf` | a newline added to every message not ending with one |
| `lfdot` | `lf`, and a line added after a last line beginning with `.` |
| `clean` | empty and headerless messages removed, and every header ended with an empty line |

`--dump FILE` appends the mismatches to `FILE` for `classify.py`.

## Expected results

Against the C mewl of 464013a built with iconv and utf8proc 2.10
(without `--enable-utf8proc-char-fold`), mewl-rs-compat differs in 12
of the 7320 cases, all in the `mime` suite and for one reason:
charsets are converted by encoding_rs, not by iconv, so 0x8160 of
Shift_JIS ("〜") is U+FF5E rather than U+301C.

```
% ./difftest.py /tmp/c/bin/mewl ../target/release/mewl-rs --name-b mewl-rs-compat
...
TOTAL: 7320 run, 12 mismatches
```

The corpus has no lone low surrogate in UTF-7, which mewl-rs takes
as invalid while glibc goes its own way (see src/utf7.rs).

## Reproducing the counts in the commit messages

Build the C mewl of each revision named below, and mewl-rs of each
commit of the series (here `rs/N` is mewl-rs built from the N-th
commit after main). The commands are run in this directory.

| commit | command | result |
|---|---|---|
| 1. port mewl to Rust | `./difftest.py --suite base C-5cb062e rs/1` | 3996 run, 0 mismatches |
| 2. restructure as idiomatic Rust | `./difftest.py --suite base C-5cb062e rs/2` | 3996 run, 0 mismatches |
| 3. decode header values as mewl-rs-utf8 | `./difftest.py --suite base C-5cb062e rs/3` | 3996 run, 0 mismatches |
| 4. follow main in field names and decoding | `./difftest.py --suite base,mime C-fd5cdab rs/4` | 5223 run, 18 mismatches |
| 5. reshape into modules, act by the name | `./difftest.py --suite base,mime rs/4 rs/5 --name-b mewl-rs-compat` | 5223 run, 0 mismatches |
| | `./difftest.py --suite base,mime rs/4 rs/5 --name-a mewl-rs-utf8 --name-b mewl-rs` | 5223 run, 0 mismatches |
| | `./difftest.py --suite base,mime C-fd5cdab rs/5 --name-b mewl-rs-compat` | 5223 run, 18 mismatches |
| 6. keep the last line, parse patterns strictly | `./difftest.py --suite base,mime rs/5 rs/6 --name-a mewl-rs --name-b mewl-rs` | 5223 run, 1138 mismatches |
| | the same with `--corpus-a lfdot --corpus-b lfdot --dump 6.jsonl`, then `./classify.py strict 6.jsonl --a rs/5 --name-a mewl-rs --corpus-a lfdot` | 272: refused 270, joined 2 |
| | the same with `--corpus-a lf --dump 6lf.jsonl`, then `./classify.py strict 6lf.jsonl --a rs/5 --name-a mewl-rs --corpus-a lf` | 615: last line 343, refused 270, joined 1, joined and last line 1 |
| | `./difftest.py --suite wellformed rs/6 rs/6 --name-a mewl-rs-compat --name-b mewl-rs` | 3000 run, 0 mismatches |
| 7. fix the bugs that need no change of Mew | `./difftest.py --suite base,mime rs/6 rs/7 --name-a mewl-rs --name-b mewl-rs` | 5223 run, 1021 mismatches |
| | the same with `--corpus-a clean --corpus-b clean --dump 7.jsonl`, then `./classify.py bugfix 7.jsonl` | 423: invalid number 52, invalid range 43, `-d` without ":" 50, 0xff in pattern 260, NUL in a list 16, `-h`/`-:` 2 |
| 8. follow main in Unicode matching | `./difftest.py --suite base,mime,uni C-868ed6f rs/8 --name-b mewl-rs-compat` | 6849 run, 24 mismatches |
| | `./difftest.py --suite base,mime,uni rs/7 rs/8 --name-a mewl-rs --name-b mewl-rs --dump 8.jsonl`, then `./classify.py pick 8.jsonl` | 253: all picks on a field |
| 10. convert UTF-7 as glibc does | `./difftest.py C-868ed6f rs/10 --name-b mewl-rs-compat` | 7320 run, 12 mismatches |
| | `./difftest.py --suite utf7 C-868ed6f rs/10 --name-b mewl-rs-compat` | 471 run, 0 mismatches |
| | `./difftest.py rs/9 rs/10 --name-a mewl-rs-compat --name-b mewl-rs-compat` | 7320 run, 196 mismatches: 12 in `+mime`, 184 in `+utf7` |
| | the same with `--name-a mewl-rs --name-b mewl-rs` | 7320 run, 240 mismatches: 20 in `+mime`, 220 in `+utf7` |
| 11. convert UTF-7 as the RFCs specify in mewl-rs | `./difftest.py rs/10 rs/11 --name-a mewl-rs-compat --name-b mewl-rs-compat` | 7320 run, 0 mismatches |
| | the same with `--name-a mewl-rs --name-b mewl-rs` | 7320 run, 122 mismatches, all in `+utf7` |
| | `./difftest.py C-868ed6f rs/11 --name-b mewl-rs-compat` | 7320 run, 12 mismatches |
| 12. replace invalid sequences with U+FFFD in mewl-rs | `./difftest.py rs/11 rs/12 --name-a mewl-rs-compat --name-b mewl-rs-compat` | 7320 run, 0 mismatches |
| | the same with `--name-a mewl-rs --name-b mewl-rs` | 7320 run, 20 mismatches: +mime/26 and +utf7/20, the messages with an invalid byte |
| 14. follow main in reading no data left by the previous message | `./difftest.py C-464013a rs/14 --name-b mewl-rs-compat`, also with `--corpus-a lf --corpus-b lf` | 7320 run, 12 mismatches |
| | `./difftest.py rs/13 rs/14 --name-a mewl-rs-compat --name-b mewl-rs-compat` | 7320 run, 182 mismatches, as many as between C-868ed6f and C-464013a: +inbox/10 (empty) and +inbox/23 (`Subj`) |
| | the same with `--name-a mewl-rs --name-b mewl-rs` | 7320 run, 0 mismatches |

mewl-rs-compat changes only in steps 4, 8, and 14, which follow main,
and in step 10, which decodes UTF-7; in the other steps it is the same
as before, e.g. 0 mismatches between rs/6 and rs/7 both invoked as
mewl-rs-compat.
