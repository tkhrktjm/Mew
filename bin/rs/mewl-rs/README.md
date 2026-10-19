# mewl-rs

A Rust port of `mewl`, the message scanner for Mew.

## Build

```
% cargo build --release
```

The binary is `target/release/mewl-rs`. `cargo install --path .`
installs it.

What it does depends on the name it is invoked by:

- `mewl-rs`: decodes MIME-encoded header values more than `mewl`.
- any other name, e.g. `mewl-rs-compat`: the same as `mewl`.

So make a link, symbolic or hard, for the other name:

```
% ln -s mewl-rs mewl-rs-compat
```

A link named `mewl` makes it a drop-in replacement for `mewl`.

## mewl-rs

Invoked as `mewl-rs`, it decodes more than `mewl` does:

- sort (`-d field`): the value printed is decoded.
- pick (`-p head=val`): the whole header is decoded, field by field.
- An encoded-word is decoded even if it is not delimited by white
  space, as in `Re:=?UTF-8?B?...?=`.
- Adjacent encoded-words in the same charset are converted together,
  so that a multibyte character split between them is restored.
- Raw ISO-2022-JP in a header, which old Japanese mailers wrote
  without encoding, is converted to UTF-8.
- A value in UTF-8 matches ignoring case (`key=val`). `mewl` on glibc
  never matches a value beginning with a byte >= 0x80 ignoring case,
  so `subject=日本語` finds nothing there.

It also fixes some bugs of `mewl`:

- The last line of a message not ending with a newline is kept.
  `mewl` overwrites its last byte with a newline, so `last` comes out
  as `las`. Likewise, the last line of the body beginning with `.` is
  escaped as the other lines are.
- A pick pattern is parsed strictly. Words not quoted after a value
  are a part of the value, so `subject=foo bar`, which Mew passes for
  `subject='foo bar'`, finds `foo bar` while `mewl` finds `foo`. A
  pattern which is not one expression, such as `a=b c=d`, `(a=b c)`,
  or `abc`, is an error ("broken pattern."), while `mewl` silently
  evaluates its first part. A word followed by `=` is the key of
  another comparison, not a part of the value.
- Nothing beyond a message is read. `mewl` reads what the previous
  message left in its buffer, so an empty message may match
  `head=val`, and a short message may show a field which it lacks.
- A message beginning with an empty line has an empty header. `mewl`
  takes its body for the header.
- A line with NUL in the list of `-i` does not hide the next line.
- A number in a range and of `-c` and `-l` must be a decimal number;
  `mewl` reads garbage as 0 and wraps a negative number around, so
  `last:-3` means all. The error is "invalid range" or "invalid
  number".
- A file whose number is too large for `unsigned int` is not a
  message. A message must be a regular file.
- The value of `-d key` without `:` does not begin with `:`.
- 0xFF in a pattern is a byte, not the end of the pattern.
- `-:` is an unknown option, and the help shows the argument of `-x`.

Bugs that need Mew to change are not fixed: for instance, Mew does not
see the exit status nor the error messages of `mewl`, and passes a
value with `&`, `|`, `(`, `)`, `!`, or `"` as it is.

Otherwise it is the same as `mewl-rs-compat`. The output of scan is
the same as `mewl`'s since Mew decodes it. Tell Mew to use it:

```
(setq mew-prog-mewl "mewl-rs")
```

## mewl-rs-compat

Invoked as `mewl-rs-compat`, or by any name but `mewl-rs`, its input,
output, and options are the same as `mewl`'s (see `mewl(1)`). As
`mewl` built with iconv, encoded-words of RFC 2047 in a field body are
decoded into UTF-8 when picking (`-p key=val`).

Charsets are converted by encoding_rs (the Encoding Standard), not by
iconv, so a few results differ from `mewl`'s:

- 0x8160 of Shift_JIS ("〜") is U+FF5E, while glibc gives U+301C.
- An encoded-word in a charset unknown to the Encoding Standard,
  such as UTF-7, is not decoded.
