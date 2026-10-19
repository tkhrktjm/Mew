# mewl-rs

A Rust port of `mewl`, the message scanner for Mew.

## Build

```
% cargo build --release
```

The binaries are `target/release/mewl-rs` and
`target/release/mewl-rs-utf8`. `cargo install --path .` installs both.

## mewl-rs

Invoked as `mewl-rs`, its input, output, and options are the same as
`mewl`'s (see `mewl(1)`). As `mewl` built with iconv, encoded-words
of RFC 2047 in a field body are decoded into UTF-8 when picking
(`-p key=val`).

Charsets are converted by encoding_rs (the Encoding Standard), not by
iconv, so a few results differ from `mewl`'s:

- 0x8160 of Shift_JIS ("〜") is U+FF5E, while glibc gives U+301C.
- An encoded-word in a charset unknown to the Encoding Standard,
  such as UTF-7, is not decoded.

## mewl-rs-utf8

Invoked as `mewl-rs-utf8`, it decodes more than `mewl` does:

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

The output of scan is the same as `mewl`'s since Mew decodes it.

`mewl-rs` invoked by the name `mewl-rs-utf8`, e.g. through a link, is
the same. Tell Mew to use it:

```
(setq mew-prog-mewl "mewl-rs-utf8")
```
