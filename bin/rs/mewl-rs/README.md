# mewl-rs

A Rust port of `mewl`, the message scanner for Mew.

A Japanese translation is in [README.ja.md](README.ja.md).

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

## Testing

`cargo test` runs the unit tests and the tests of the binary.
`difftest/` compares mewl-rs with the C mewl over a corpus of
messages; see difftest/README.md.

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
- A charset is named as the Encoding Standard names it, or else as
  glibc does, such as `cp949`, `cp936`, `latin9`, and `ujis`, and is
  converted by encoding_rs as the Encoding Standard specifies: e.g.
  ISO-8859-1 and US-ASCII are windows-1252.
- ISO-2022-JP knows every set of ISO-2022-JP-2 (RFC 1554) whatever
  the charset is named, since an escape sequence tells which set
  follows: JIS X 0212 as in "Gojō" of some User-Agent, GB 2312, KS C
  5601, and ISO 8859-1 and 8859-7 by `ESC N`. ISO-2022-JP-1 (RFC 2237)
  is known too. JIS X 0208 has the extensions of Windows, such as
  "①". An escape sequence not known, e.g. of JIS X 0213 in
  ISO-2022-JP-3, is skipped, and the characters of its set are
  U+FFFD.
- An invalid byte sequence in the charset of an encoded-word, or in
  raw ISO-2022-JP, is replaced with U+FFFD (REPLACEMENT CHARACTER),
  one for each maximal subpart as the Encoding Standard does, while
  `mewl` keeps the bytes as they are. So a decoded value is always
  UTF-8, and a field with a broken character is still matched with
  case folding and NFC rather than byte by byte. Raw 8-bit text which
  is not encoded is left as it is, since its charset is unknown.
- UTF-7 and UTF-7-IMAP are converted as RFC 2152 and RFC 3501
  specify, not as glibc does. A shifted sequence (from `+`, or `&`, to
  the end of its base64) is decoded only if it is well-formed as a
  whole: its surrogates are paired, the bits left over are fewer than
  6 and zero, and, for UTF-7-IMAP, it ends with `-`, encodes no
  printable US-ASCII, and does not follow another one right away.
  Otherwise it is kept as it is, so that what is shown is either the
  text or what was sent, never characters made of a broken sequence.
  A US-ASCII character which may not stand for itself, such as `~` in
  UTF-7, is kept as well; a byte not in US-ASCII is U+FFFD.

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
- `-:` is an unknown option, and the help shows the argument of `-x`
  and lists `key==val` and `key!==val` among the patterns.

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
`mewl` built with iconv and utf8proc (but not with
`--enable-utf8proc-char-fold`), when picking (`-p key=val`):

- encoded-words of RFC 2047 in a field body are decoded into UTF-8;
- `=` and `!=` compare a field body with the value after Unicode full
  case folding and canonical composition (NFC), so that "Straße"
  matches "STRASSE"; `==` and `!==` after NFC only. Width is not
  folded. When either is not valid UTF-8, they are compared byte by
  byte, ignoring the case of ASCII letters only. The key `head` is
  compared as it is.

A charset is named as `mewl` with glibc names it. `mewl` renames a
few MIME charset names for iconv, such as `ks_c_5601-1987` to `CP949`,
and retries a few with a superset when the conversion fails, such as
`shift_jis` with `CP932`. glibc ignores the case of a name and the
characters in it but letters, digits, and `_-.,:`, so that `utf~8` is
UTF-8, and does not know some names of the Encoding Standard, such as
`x-sjis` and `gb_2312`. `tools/glibc-charsets.py` makes the list of the
names glibc knows, `src/charset/glibc.rs`.

A single-byte charset is converted by the table of glibc, which the
script also takes from glibc: so ISO-8859-1 is not windows-1252, as in
the Encoding Standard, but has the C1 controls, US-ASCII has no byte
over 0x7F, and a byte undefined in windows-1252 is invalid. Shift_JIS,
EUC-JP, CP932, and eucJP-ms are converted as glibc does too, by
encoding_rs patched where they differ: in Shift_JIS and EUC-JP,
0x2141 of JIS X 0208 ("〜") is U+301C (WAVE DASH), not U+FF5E as in
Windows, and so are five more characters, the NEC special characters
and the IBM extensions are invalid, so that `mewl` retries with CP932
or eucJP-ms, and 0x5C and 0x7E of Shift_JIS are "¥" and "‾".
ISO-2022-JP, ISO-2022-JP-2, UTF-7, and UTF-7-IMAP are converted as
glibc does, invalid sequences and the tables of glibc included, not as
the Encoding Standard, which knows only ISO-2022-JP, with the table of
Windows: e.g. "①" is not in ISO-2022-JP, and ISO-2022-JP-1, which
glibc lacks, is not decoded. The other
charsets are converted by encoding_rs (the Encoding Standard), not by
iconv. Unicode is normalized by unicode-normalization and caseless,
not by utf8proc. So a few results differ from `mewl`'s:

- The other multibyte charsets, such as GBK, Big5, and EUC-KR, have
  the tables of the Encoding Standard, which are not those of glibc.
- An encoded-word in a charset which glibc knows but encoding_rs and
  mewl-rs do not, such as UTF-32, UCS-4, ISO-2022-KR, ISO-2022-CN,
  JOHAB, and EBCDIC, is not decoded.
- UTF-16 and UCS-2 are UTF-16LE (UCS-2BE is UTF-16BE), without the
  byte order mark which glibc takes for UTF-16 and seems to keep for
  the next encoded-words, and an invalid sequence is kept otherwise.
- windows-1255 and windows-1258 are converted without the composition
  of glibc, by which `mewl` loses the last character of an
  encoded-word that may be composed with what follows, as it never
  flushes iconv: "café" is "caf".
- ISO-2022-JP-3 is converted as ISO-2022-JP, without JIS X 0213 and
  the Katakana of JIS X 0201, which glibc knows.
- A lone low surrogate in UTF-7 makes the character completing it
  invalid, while glibc passes it to the next step of the conversion,
  which refuses it in a way that depends on the internals of iconv.
- Characters new in a version of Unicode which one of them does not
  know yet may be normalized differently.

Two more things differ from `mewl`, which Mew does not depend on:

- `-v` prints the version of mewl-rs, not that of Mew.
- `mewl` duplicates stdin as file descriptor 3, which the command of
  `-e` inherits; mewl-rs does not.
