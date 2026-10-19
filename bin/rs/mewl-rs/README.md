# mewl-rs

A Rust port of `mewl`, the message scanner for Mew.

## Build

```
% cargo build --release
```

The binary is `target/release/mewl-rs`.

## mewl-rs

Invoked as `mewl-rs`, its input, output, and options are the same as
`mewl`'s (see `mewl(1)`).

## mewl-rs-utf8

Invoked as `mewl-rs-utf8`, MIME-encoded header values (RFC 2047) are
decoded and converted to UTF-8 for:

- sort (`-d field`): the value printed, and
- pick (`-p pattern`): the header searched, so that a pattern in
  Japanese, for instance, matches an encoded `Subject:`.

The output of scan is the same as `mewl`'s since Mew decodes it.

Make a link with this name:

```
% ln -s mewl-rs mewl-rs-utf8
```

and tell Mew to use it:

```
(setq mew-prog-mewl "mewl-rs-utf8")
```
