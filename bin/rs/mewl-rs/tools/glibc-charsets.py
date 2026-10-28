#!/usr/bin/env python3
"""Generate src/charset/glibc.rs from the iconv of glibc.

Usage: glibc-charsets.py [OUTPUT]

mewl converts charsets with iconv_open(3). mewl-rs-compat knows a
charset name as glibc does: this lists the names glibc knows for each
converter that mewl-rs-compat emulates, as glibc normalizes them. The
names come from `iconv -l` and the converter of each from the
gconv-modules files (the converters built into glibc, US-ASCII and
UTF-8, are listed here). For a single-byte charset, it also takes the
table of its upper half from glibc. Run it where glibc is the C
library; the output names the version of glibc.
"""

import ctypes
import ctypes.util
import glob
import os
import subprocess
import sys

GCONV_DIRS = ["/usr/lib/x86_64-linux-gnu/gconv", "/usr/lib/aarch64-linux-gnu/gconv",
              "/usr/lib64/gconv", "/usr/lib/gconv"]

# The converters built into glibc (iconv/gconv_builtin.h), by name.
BUILTIN = {
    "ANSI_X3.4-1968": ["ANSI_X3.4-1968", "ANSI_X3.4-1986", "ANSI_X3.4", "ASCII", "CP367",
                       "CSASCII", "IBM367", "ISO-IR-6", "ISO646-US", "ISO_646.IRV:1991",
                       "OSF00010020", "US-ASCII", "US"],
    "UTF-8": ["UTF-8", "UTF8", "ISO-10646/UTF-8/", "ISO-10646/UTF8/", "OSF05010001",
              "ISO-IR-193"],
    # sorted by byte order in builtin_converters()
    "UCS-2": ["UCS-2", "UCS2", "ISO-10646/UCS2/", "OSF00010100", "OSF00010101",
              "OSF00010102", "UCS-2BE", "UNICODEBIG", "UCS-2LE", "UNICODELITTLE"],
    # not emulated
    "UCS-4": ["UCS-4", "UCS4", "ISO-10646", "ISO-10646/UCS4/", "10646-1:1993",
              "10646-1:1993/UCS4/", "CSUCS4", "OSF00010104", "OSF00010105", "OSF00010106",
              "UCS-4BE", "UCS-4LE", "WCHAR_T"],
}

# The converters emulated, and how: by an encoding of encoding_rs, which
# is what mewl-rs takes the names for, by a decoder of mewl-rs, or, for
# a single-byte charset, by the table of glibc (with the encoding of
# encoding_rs for mewl-rs). CP1255 and CP1258 of glibc compose
# characters, which a table does not.
CONVERTERS = {
    "ANSI_X3.4-1968": ("Single", "WINDOWS_1252"),
    "ISO-8859-1": ("Single", "WINDOWS_1252"),
    "ISO-8859-2": ("Single", "ISO_8859_2"),
    "ISO-8859-3": ("Single", "ISO_8859_3"),
    "ISO-8859-4": ("Single", "ISO_8859_4"),
    "ISO-8859-5": ("Single", "ISO_8859_5"),
    "ISO-8859-6": ("Single", "ISO_8859_6"),
    "ISO-8859-7": ("Single", "ISO_8859_7"),
    "ISO-8859-8": ("Single", "ISO_8859_8"),
    "ISO-8859-9": ("Single", "WINDOWS_1254"),
    "ISO-8859-10": ("Single", "ISO_8859_10"),
    "ISO-8859-11": ("Single", "WINDOWS_874"),
    "ISO-8859-13": ("Single", "ISO_8859_13"),
    "ISO-8859-14": ("Single", "ISO_8859_14"),
    "ISO-8859-15": ("Single", "ISO_8859_15"),
    "ISO-8859-16": ("Single", "ISO_8859_16"),
    "TIS-620": ("Single", "WINDOWS_874"),
    "IBM874": ("Single", "WINDOWS_874"),
    "KOI-8": ("Single", "KOI8_R"),
    "KOI8-R": ("Single", "KOI8_R"),
    "KOI8-U": ("Single", "KOI8_U"),
    "KOI8-RU": ("Single", "KOI8_U"),
    "IBM866": ("Single", "IBM866"),
    "MACINTOSH": ("Single", "MACINTOSH"),
    "CP1250": ("Single", "WINDOWS_1250"),
    "CP1251": ("Single", "WINDOWS_1251"),
    "CP1252": ("Single", "WINDOWS_1252"),
    "CP1253": ("Single", "WINDOWS_1253"),
    "CP1254": ("Single", "WINDOWS_1254"),
    "CP1255": "Encoding(&WINDOWS_1255_INIT)",
    "CP1256": ("Single", "WINDOWS_1256"),
    "CP1257": ("Single", "WINDOWS_1257"),
    "CP1258": "Encoding(&WINDOWS_1258_INIT)",
    "UTF-8": "Encoding(&UTF_8_INIT)",
    "UTF-16": "Encoding(&UTF_16LE_INIT)",
    "UNICODE": "Encoding(&UTF_16LE_INIT)",
    "UTF-16LE": "Encoding(&UTF_16LE_INIT)",
    "UTF-16BE": "Encoding(&UTF_16BE_INIT)",
    "UCS-2LE": "Encoding(&UTF_16LE_INIT)",
    "UCS-2BE": "Encoding(&UTF_16BE_INIT)",
    "SJIS": "Encoding(&SHIFT_JIS_INIT)",
    "CP932": "Encoding(&SHIFT_JIS_INIT)",
    "EUC-JP": "Encoding(&EUC_JP_INIT)",
    "EUC-JP-MS": "Encoding(&EUC_JP_INIT)",
    "BIG5": "Encoding(&BIG5_INIT)",
    "BIG5HKSCS": "Encoding(&BIG5_INIT)",
    "EUC-CN": "Encoding(&GBK_INIT)",
    "GBK": "Encoding(&GBK_INIT)",
    "GB18030": "Encoding(&GB18030_INIT)",
    "EUC-KR": "Encoding(&EUC_KR_INIT)",
    "UHC": "Encoding(&EUC_KR_INIT)",
    "ISO-2022-JP": "Iso2022Jp(Jp::Jp)",
    "ISO-2022-JP-2": "Iso2022Jp(Jp::Jp2)",
    "ISO-2022-JP-3": "Iso2022Jp(Jp::Jp3)",
    "UTF-7": "Utf7(U7::Utf7)",
    "UTF-7-IMAP": "Utf7(U7::Imap)",
}


def normalize(name):
    """A name as glibc compares it: upper case, without the characters
    other than letters, digits, and "_-.,:" (iconv/gconv_charset.c)."""
    return "".join(c for c in name.upper() if c.isascii() and (c.isalnum() or c in "_-.,:"))


def gconv_modules():
    """{name: converter} from the gconv-modules files."""
    for d in GCONV_DIRS:
        if os.path.exists(os.path.join(d, "gconv-modules")):
            break
    else:
        sys.exit("gconv-modules not found")
    files = [os.path.join(d, "gconv-modules")] + sorted(glob.glob(os.path.join(d, "gconv-modules.d", "*.conf")))
    conv = {}
    for f in files:
        for line in open(f):
            w = line.split()
            if len(w) >= 3 and w[0] == "alias":
                conv[w[1].rstrip("/").upper()] = w[2].rstrip("/").upper()
            elif len(w) >= 4 and w[0] == "module" and w[2] == "INTERNAL":
                conv[w[1].rstrip("/").upper()] = w[1].rstrip("/").upper()
    for c, names in BUILTIN.items():
        for n in names:
            conv[n.rstrip("/").upper()] = c if c != "UCS-2" else ucs2_order(n)
    return conv


def iconv(charset, data):
    """`data` converted from `charset` to UTF-8, or None."""
    p = subprocess.run(["iconv", "-f", charset, "-t", "UTF-8"], input=data, capture_output=True)
    return p.stdout if p.returncode == 0 else None


def ucs2_order(name):
    """UCS-2LE or UCS-2BE, by what UCS-2 of glibc named `name` is: the
    byte order of the host, or big-endian."""
    return {b"A": "UCS-2LE", "\u4100".encode(): "UCS-2BE"}[iconv(name, b"A\0")]


def single_byte(charset):
    """The upper half of a single-byte charset: 128 code points, 0 for an
    invalid byte. The lower half has to be US-ASCII, and the charset
    has to be stateless."""
    table = []
    for b in range(256):
        out = iconv(charset, bytes([b]))
        c = ord(out.decode()) if out else 0
        if b < 0x80 and c != b:
            sys.exit(f"{charset}: 0x{b:02X} is not US-ASCII")
        if b >= 0x80:
            table.append(c)
    valid = bytes(b for b in range(256) if b < 0x80 or table[b - 0x80])
    if iconv(charset, valid).decode() != "".join(chr(b) if b < 0x80 else chr(table[b - 0x80]) for b in valid):
        sys.exit(f"{charset} is not stateless")
    return table


def glibc_version():
    libc = ctypes.CDLL(ctypes.util.find_library("c"))
    libc.gnu_get_libc_version.restype = ctypes.c_char_p
    return libc.gnu_get_libc_version().decode()


def main():
    out = sys.argv[1] if len(sys.argv) > 1 else os.path.join(os.path.dirname(__file__), "..", "src", "charset", "glibc.rs")
    listed = subprocess.run(["iconv", "-l"], capture_output=True, text=True, check=True).stdout
    names = [n.strip().rstrip("/") for n in listed.replace("\n", ",").split(",") if n.strip()]
    conv = gconv_modules()
    table = {}
    for n in names:
        c = conv.get(n.upper())
        if c is None:
            sys.exit(f"no converter for {n}")
        if c in CONVERTERS and "/" not in n:  # "/" is not in a MIME charset name
            key = normalize(n)
            if table.setdefault(key, c) != c:
                sys.exit(f"{n} names both {table[key]} and {c}")
    missing = set(CONVERTERS) - set(table.values())
    if missing:
        sys.exit(f"converters not found: {sorted(missing)}")
    lines = [
        f"//! The charset names of glibc {glibc_version()}, generated by",
        "//! tools/glibc-charsets.py; do not edit.",
        "",
        "use encoding_rs::*;",
        "",
        "use super::Converter::{self, *};",
        "use super::SingleByte;",
        "use crate::iso2022jp::Variant as Jp;",
        "use crate::utf7::Variant as U7;",
        "",
        "/// The names of the converters emulated, normalized as glibc does",
        "/// (see [`super::normalize`]) and sorted, with their converters.",
        "pub(super) static NAMES: &[(&[u8], Converter)] = &[",
    ]
    def rust(c):
        how = CONVERTERS[c]
        return how if isinstance(how, str) else f"Single(&{ident(c)})"

    def ident(c):
        return "".join(x if x.isalnum() else "_" for x in c)

    for key in sorted(table):
        lines.append(f'    (b"{key}", {rust(table[key])}), /* {table[key]} */')
    lines.append("];")
    for c, how in CONVERTERS.items():
        if isinstance(how, str):
            continue
        upper = single_byte(c)
        lines += ["", f"/// {c}", f"static {ident(c)}: SingleByte = SingleByte {{",
                  f"    encoding: &{how[1]}_INIT,", "    upper: ["]
        for row in range(0, 128, 8):
            lines.append("        " + " ".join(f"0x{x:04X}," for x in upper[row:row + 8])
                         + f" /* {0x80 + row:02X} */")
        lines += ["    ],", "};"]
    with open(out, "w") as f:
        f.write("\n".join(lines) + "\n")
    print(f"{out}: {len(table)} names of {len(CONVERTERS)} converters")


if __name__ == "__main__":
    main()
