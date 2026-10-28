#!/usr/bin/env python3
"""Compare the decoding of mewl-rs with that of Mew.

Usage: mewcheck.py MEWL-RS [--elisp DIR] [--work DIR] [--charset NAME ...]
       mewcheck.py MEWL-RS --guess [--locale LOCALE]

mewl-rs decodes a header value for Mew, which shows what it decodes by
itself: mewcheck.py takes the charsets of Mew's
mew-cs-database-for-decoding, makes an encoded-word of each character
code of each charset (every byte, every pair of bytes in the range of
a double-byte charset, and every character of JIS X 0208 and 0212 in
ISO-2022-JP), and compares what `mewl-rs -d subject:` prints with what
Mew's mew-header-decode makes of it in Emacs (mewcheck.el). Control
characters, which both remove or show as they are, are not compared.

Per charset, it counts the codes decoded the same and differently,
those that Emacs keeps as raw bytes, as invalid, while mewl-rs replaces
them with U+FFFD or decodes them, those that Emacs decodes into
characters without Unicode, and those that it decodes into characters
of the Private Use Area or unassigned (by the Unicode of Python) while
mewl-rs does not.

With --guess, it compares how they guess a charset, both run in LOCALE
(ja_JP.UTF-8 by default): texts in Japanese and in Latin letters,
encoded in UTF-8, ISO-2022-JP, EUC-JP, Shift_JIS, ISO-8859-1, and
windows-1252, and random bytes, each in an encoded-word in a charset
Mew does not know.
"""

import argparse
import base64
import os
import shutil
import subprocess
import sys
import unicodedata
from collections import Counter, defaultdict

HERE = os.path.dirname(os.path.abspath(__file__))

ISO2022 = {"iso-2022-jp": [b"\x1b$B"], "iso-2022-jp-2": [b"\x1b$B", b"\x1b$(D", b"\x1b$A", b"\x1b$(C"],
           "iso-2022-jp-3": [b"\x1b$B"]}
EUC = ("euc-jp", "euc-kr", "ks_c_5601-1987", "gb2312", "cn-gb")
LEADS = ("shift_jis", "windows-31j", "big5", "cn-big5", "gbk", "gb18030")


def private_or_unassigned(code):
    """Whether Emacs takes a code for a character that nobody types."""
    return 0xe000 <= code <= 0xf8ff or code >= 0xf0000 or unicodedata.category(chr(code)) == "Cn"


def codes(charset):
    """The codes of `charset` to decode, as bytes."""
    if charset in ISO2022:
        return [esc + bytes([a, b]) + b"\x1b(B" for esc in ISO2022[charset]
                for a in range(0x21, 0x7f) for b in range(0x21, 0x7f)]
    singles = [bytes([b]) for b in range(0x20, 0x100)]
    if charset in EUC:
        return singles + [bytes([a, b]) for a in range(0xa1, 0xff) for b in range(0xa1, 0xff)]
    if charset in LEADS:
        return singles + [bytes([a, b]) for a in range(0x81, 0xff) for b in range(0x40, 0xff) if b != 0x7f]
    if charset.startswith("utf-") or "2022" in charset or charset.startswith("hz"):
        return None
    return singles


TEXTS = ["日本語", "件名の例です", "Re: 会議の議事録（第2回）", "〜から〜まで", "①②③と㈱", "ｶﾀｶﾅとカタカナ", "表示テスト",
         "Gojō", "café au lait", "Straße", "naïve résumé", "“quoted” – dash", "abc", "©®", "ー・「」"]
CODECS = ["utf-8", "iso2022_jp", "euc_jp", "shift_jis", "latin-1", "cp1252"]


def guess_inputs():
    """Texts in several charsets, and random bytes, as bytes."""
    import random
    seen = set()
    for text in TEXTS:
        for codec in CODECS:
            try:
                data = text.encode(codec)
            except UnicodeEncodeError:
                continue
            if data not in seen:
                seen.add(data)
                yield data
    rnd = random.Random(9)
    for _ in range(1000):
        yield bytes(rnd.choice([rnd.randrange(0x20, 0x7f), rnd.randrange(0x80, 0x100)]) for _ in range(rnd.randint(1, 6)))


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("mewl_rs", help="the mewl-rs binary")
    p.add_argument("--elisp", default=os.path.join(HERE, "..", "..", "..", "..", "elisp"), help="Mew's elisp")
    p.add_argument("--work", default=os.path.join(HERE, "work", "mewcheck"), help="where the messages are made")
    p.add_argument("--charset", action="append", help="check only these charsets")
    p.add_argument("--show", type=int, default=5, help="how many differences to print per charset")
    p.add_argument("--guess", action="store_true", help="compare how they guess a charset")
    p.add_argument("--locale", default="ja_JP.UTF-8", help="the locale for --guess")
    args = p.parse_args()

    emacs = ["emacs", "-Q", "--batch", "-L", args.elisp, "-l", os.path.join(HERE, "mewcheck.el")]
    env = dict(os.environ, LC_ALL=args.locale, LANG=args.locale) if args.guess else None
    listed = subprocess.run(emacs + ["charsets"], capture_output=True, text=True, check=True).stdout
    charsets = [line.split("\t")[0] for line in listed.splitlines()]
    if args.charset:
        charsets = [c for c in charsets if c in args.charset]
    if args.guess:
        charsets = ["x-unknown"]

    bindir = os.path.join(args.work, "bin")
    shutil.rmtree(args.work, ignore_errors=True)
    os.makedirs(bindir)
    os.symlink(os.path.abspath(args.mewl_rs), os.path.join(bindir, "mewl-rs"))
    total = Counter()
    for charset in charsets:
        inputs = list(guess_inputs()) if args.guess else codes(charset)
        if inputs is None:
            print(f"{charset}: skipped")
            continue
        folder = os.path.join(args.work, "Mail", "c")
        shutil.rmtree(folder, ignore_errors=True)
        os.makedirs(folder)
        words = [base64.b64encode(code).decode() for code in inputs]
        for i, word in enumerate(words, 1):
            with open(os.path.join(folder, str(i)), "w") as f:
                f.write(f"Subject: =?{charset}?B?{word}?=\n\n")
        out = subprocess.run([os.path.join(bindir, "mewl-rs"), "-b", os.path.join(args.work, "Mail"), "-l", "0",
                              "-d", "subject:", "+c"], capture_output=True, check=True, env=env).stdout
        rs = {}
        for line in out.decode("utf-8", "surrogateescape").split("\n"):
            num, _, value = line.partition(": ")
            if num.isdigit():
                rs[int(num)] = value
        mew = subprocess.run(emacs, input="".join(f"{charset}\t{w}\n" for w in words), capture_output=True,
                             text=True, check=True, env=env).stdout.split("\n")
        count = Counter()
        diffs = []
        for i, code in enumerate(inputs, 1):
            m = [c for c in mew[i - 1].split() if c in "RXU" or int(c, 16) >= 0x20 and int(c, 16) != 0x7f]
            r = [f"{ord(c):X}" for c in rs.get(i, "")]
            if m == ["U"]:
                kind = "unknown to Mew"
            elif "X" in m:
                kind = "no Unicode in Emacs"
            elif "R" in m:
                kind = "invalid" if "FFFD" in r else "invalid to Mew only"
            elif m == r:
                kind = "same"
            elif any(private_or_unassigned(int(c, 16)) for c in m):
                kind = "private or unassigned to Mew"
            else:
                kind = "differ"
                diffs.append((code, m, r))
            count[kind] += 1
        total.update(count)
        print(f"{charset}: " + ", ".join(f"{v} {k}" for k, v in sorted(count.items())))
        for code, m, r in diffs[:args.show]:
            print(f"    {code.hex()}: Mew {' '.join(m)}, mewl-rs {' '.join(r)}")
    print("TOTAL: " + ", ".join(f"{v} {k}" for k, v in sorted(total.items())))


if __name__ == "__main__":
    main()
