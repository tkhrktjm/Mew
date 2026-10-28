#!/usr/bin/env python3
"""Generate src/charset/glibc.rs from the iconv of glibc.

Usage: glibc-charsets.py [OUTPUT]

mewl converts charsets with iconv_open(3). mewl-rs-compat knows a
charset name as glibc does: this lists the names glibc knows for each
converter that mewl-rs-compat emulates, as glibc normalizes them. The
names come from `iconv -l` and the converter of each from the
gconv-modules files (the converters built into glibc, US-ASCII and
UTF-8, are listed here). Run it where glibc is the C library; the
output names the version of glibc.
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
# is what mewl-rs takes the names for, or by a decoder of mewl-rs.
CONVERTERS = {
    "ANSI_X3.4-1968": "Encoding(&WINDOWS_1252_INIT)",
    "ISO-8859-1": "Encoding(&WINDOWS_1252_INIT)",
    "ISO-8859-2": "Encoding(&ISO_8859_2_INIT)",
    "ISO-8859-3": "Encoding(&ISO_8859_3_INIT)",
    "ISO-8859-4": "Encoding(&ISO_8859_4_INIT)",
    "ISO-8859-5": "Encoding(&ISO_8859_5_INIT)",
    "ISO-8859-6": "Encoding(&ISO_8859_6_INIT)",
    "ISO-8859-7": "Encoding(&ISO_8859_7_INIT)",
    "ISO-8859-8": "Encoding(&ISO_8859_8_INIT)",
    "ISO-8859-9": "Encoding(&WINDOWS_1254_INIT)",
    "ISO-8859-10": "Encoding(&ISO_8859_10_INIT)",
    "ISO-8859-11": "Encoding(&WINDOWS_874_INIT)",
    "ISO-8859-13": "Encoding(&ISO_8859_13_INIT)",
    "ISO-8859-14": "Encoding(&ISO_8859_14_INIT)",
    "ISO-8859-15": "Encoding(&ISO_8859_15_INIT)",
    "ISO-8859-16": "Encoding(&ISO_8859_16_INIT)",
    "TIS-620": "Encoding(&WINDOWS_874_INIT)",
    "IBM874": "Encoding(&WINDOWS_874_INIT)",
    "KOI-8": "Encoding(&KOI8_R_INIT)",
    "KOI8-R": "Encoding(&KOI8_R_INIT)",
    "KOI8-U": "Encoding(&KOI8_U_INIT)",
    "KOI8-RU": "Encoding(&KOI8_U_INIT)",
    "IBM866": "Encoding(&IBM866_INIT)",
    "MACINTOSH": "Encoding(&MACINTOSH_INIT)",
    "CP1250": "Encoding(&WINDOWS_1250_INIT)",
    "CP1251": "Encoding(&WINDOWS_1251_INIT)",
    "CP1252": "Encoding(&WINDOWS_1252_INIT)",
    "CP1253": "Encoding(&WINDOWS_1253_INIT)",
    "CP1254": "Encoding(&WINDOWS_1254_INIT)",
    "CP1255": "Encoding(&WINDOWS_1255_INIT)",
    "CP1256": "Encoding(&WINDOWS_1256_INIT)",
    "CP1257": "Encoding(&WINDOWS_1257_INIT)",
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
        "use crate::iso2022jp::Variant as Jp;",
        "use crate::utf7::Variant as U7;",
        "",
        "/// The names of the converters emulated, normalized as glibc does",
        "/// (see [`super::normalize`]) and sorted, with their converters.",
        "pub(super) static NAMES: &[(&[u8], Converter)] = &[",
    ]
    for key in sorted(table):
        lines.append(f'    (b"{key}", {CONVERTERS[table[key]]}), /* {table[key]} */')
    lines.append("];")
    with open(out, "w") as f:
        f.write("\n".join(lines) + "\n")
    print(f"{out}: {len(table)} names of {len(CONVERTERS)} converters")


if __name__ == "__main__":
    main()
