#!/usr/bin/env python3
"""Make the corpus for difftest.py.

Usage: mkcorpus.py DIR

DIR/home/Mail gets these folders (HOME is DIR/home when mewl runs):

  inbox       edge cases of mewl: no trailing newline, an empty file, NUL,
              more than 64KB, CRLF, an empty header, bytes >= 0x80, a
              directory, a symbolic link, file names with a suffix, ...
  rand        120 random messages (seeded, so always the same)
  my folder   a folder whose name has a space
  a/b, empty  a nested folder and an empty one
  mime        encoded-words of RFC 2047 in many charsets, and broken ones
  uni         Unicode matching: case folding, NFC, width, invalid UTF-8
  utf7        encoded-words in UTF-7 and UTF-7-IMAP, valid and invalid
              but without a lone low surrogate, which glibc handles in
              a way not followed

DIR/cwd gets list1, list2 and list3, the lists for -i, and is where mewl
runs.
"""

import base64
import os
import random
import sys

# Bump this when the corpus changes, so that difftest.py makes it again.
VERSION = 2


def w(path, data):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "wb") as f:
        f.write(data)


def make_base(mail):
    """inbox, rand, my folder, a/b, and empty."""
    inbox = os.path.join(mail, "inbox")
    w(f"{inbox}/1", b"Subject: hello\nFrom: Foo <foo@example.com>\nTo: bar@example.com\nDate: Mon, 1 Jan 2024 00:00:00 +0900\nMessage-ID: <1@x>\nX-Other: zzz\n\nbody line 1\n.dot line\nbody 3\n")
    w(f"{inbox}/2.mew", b"Subject: =?UTF-8?B?44GC?=\n continued subject\n\tcontinued 2\n third\n fourth\n fifth\n sixth\nReferences: <a@x>\n <b@x>\n <c@x>\nIn-Reply-To: <c@x>\nContent-Type: text/plain; charset=utf-8\n\n" + b"".join(b"line %d\n" % i for i in range(40)))
    w(f"{inbox}/3", b"subject: lower case\nreferences: <only@x>\n\n.\n..\nend")          # no trailing newline
    w(f"{inbox}/05", b"Subject: zero prefixed\n\nbody\n")
    w(f"{inbox}/10", b"")                                                              # empty
    w(f"{inbox}/11", b"Subject: no body")                                              # no trailing newline
    w(f"{inbox}/12", b"Subject: nul\x00inside\nFrom: a\x00b\n\nbo\x00dy\n")
    w(f"{inbox}/13", b"Subject: big\n\n" + b"x" * 70000 + b"\n")                      # > 64KB
    w(f"{inbox}/14", b"Subject: crlf\r\nFrom: crlf@x\r\n\r\nbody\r\n")
    w(f"{inbox}/15", b"\nempty header\n")                                              # empty header
    w(f"{inbox}/16", b"Subject: \xe3\x81\x82\xff\x80 high bytes FOO\nX-Mew-Ref: r\n\nBODY\n")  # bytes >= 0x80, invalid UTF-8
    w(f"{inbox}/abc", b"Subject: not a message\n\n")
    w(f"{inbox}/17x", b"Subject: not a message\n\n")
    w(f"{inbox}/0", b"Subject: zero\n\n")
    os.makedirs(f"{inbox}/18", exist_ok=True)                                          # a directory
    w(f"{inbox}/19.mew.mew", b"Subject: double suffix\n\n")
    w(f"{inbox}/20.txt", b"Subject: suffix txt\n\n")
    w(f"{inbox}/21", b"Subject: twentyone\nFrom: x\n\n" + b"\n" * 30)
    w(f"{inbox}/22", b"Subject: " + b"long " * 3000 + b"\n" + b"From: after-long\n\nb\n")
    w(f"{inbox}/23", b"Subj")                                                          # shorter than a field name
    w(f"{inbox}/24", b"From: only-from\nReferences:\n <r1@x>\n\n")
    w(f"{inbox}/25", b"Subject: sp folder\n" + b"".join(b" cont %d\n" % i for i in range(10)) + b"\nbody\n")
    os.symlink("1", f"{inbox}/26")
    w(f"{mail}/my folder/1", b"Subject: in space folder\n\nhi\n")
    w(f"{mail}/my folder/2", b"Subject: second\n\nhi\n")
    w(f"{mail}/a/b/3", b"Subject: nested\nFrom: nest@x\n\nhi\n")
    os.makedirs(f"{mail}/empty", exist_ok=True)
    random.seed(42)
    names = [b"Subject:", b"From:", b"To:", b"Cc:", b"Date:", b"References:", b"X-Foo:", b"Content-Type:", b"Message-ID:", b"subject:", b"FROM:"]
    words = [b"foo", b"Bar", b"BAZ", b"qux", b"<id@x>", b".", b"..", b"", b"\xe3\x81\x82", b"\t", b"x"*100]
    for i in range(1, 121):
        hdr = b""
        for _ in range(random.randint(0, 10)):
            hdr += random.choice(names) + b" " + b" ".join(random.choices(words, k=random.randint(0, 4))) + b"\n"
            for _ in range(random.choice([0,0,0,1,2,7])):
                hdr += random.choice([b" ", b"\t"]) + b" ".join(random.choices(words, k=2)) + b"\n"
        body = b"".join(random.choice([b".", b"", b"text "]) + b" ".join(random.choices(words, k=random.randint(0,3))) + b"\n" for _ in range(random.randint(0, 30)))
        data = hdr + random.choice([b"\n", b"", b"\n\n"]) + body
        if random.random() < 0.1: data = data[:random.randint(0, len(data))]
        w(f"{mail}/rand/{i}", data)


def make_mime(mail):
    """mime: encoded-words."""
    mail = os.path.join(mail, "mime")
    os.makedirs(mail, exist_ok=True)
    def b(cs, s, enc=None):
        raw = s.encode(enc or cs) if isinstance(s, str) else s
        return b"=?" + cs.encode() + b"?B?" + base64.b64encode(raw) + b"?="
    def q(cs, raw):
        return b"=?" + cs.encode() + b"?Q?" + b"".join(b"=%02X" % c if c > 126 or c in b"=?_ " or c < 33 else bytes([c]) for c in raw) + b"?="
    subjects = [
        b("UTF-8", "日本語の件名"), b("utf-8", "Résumé ÀÉÎ"), q("ISO-8859-1", "café au lait".encode("latin1")),
        b("ISO-2022-JP", "日本語", "iso2022_jp"), b("ISO-2022-JP", "日本語", "iso2022_jp") + b"\n " + b("ISO-2022-JP", "の日本", "iso2022_jp"),
        b("Shift_JIS", "日本語〜", "cp932"), b("Shift_JIS", "日本語", "sjis"), b("EUC-JP", "日本語", "euc_jp"), b("windows-31j", "①②", "cp932"),
        b("GB2312", "中文", "gb2312"), b("Big5", "中文", "big5"), b("EUC-KR", "한국어", "euc_kr"), b("KOI8-R", "Привет", "koi8_r"),
        b"Re:" + b("UTF-8", "日本語"), b"Re: " + b("UTF-8", "日本語") + b" (2)", b("UTF-8", "日本") + b" " + b("UTF-8", "語"),
        b"=?UTF-8?B?5pel5pys?= text =?UTF-8?B?6Kqe?=", b"=?UTF-8?B?5pel5g?= =?UTF-8?B?nKzoqp4?=",  # a character split in two
        b"=?UNKNOWN?B?YWJj?=", b"=?UTF-8?B?Y?=", b"=?UTF-8?B?5pel5pys6Kqe44GuNA?=", b"=?UTF-8?B?5pel5pys6Kqe44GuNA===?=",
        b"=?UTF-8?Q?a=ZZ?=", b"=?UTF-8?Q?a?b?=", b"=?UTF-8?Q?a=0D=0A=09b=00c?=", b"=?UTF-8?B?/2Fi?=", b"=?UTF-8*ja?B?5pel5pys6Kqe?=",
        b"=?ISO-2022-JP?B?GyRCRnxLXDhs?=", b"=?US-ASCII?Q?plain_ascii?=", b"=?us-ascii?B?" + base64.b64encode(b"hi\xe9") + b"?=",
        b"a =? b ?= c", b"=?UTF-8?B??=", b"=?UTF-8?B?====?=", b"=?UTF-8?q?caf=C3=A9?=", b"=?iso-8859-15?Q?=A4uro?=", b"=?windows-1252?Q?=80uro?=",
        b"\x1b$BF|K\\8l\x1b(B raw jis", b"=?UTF-7?B?K2Y1OGc2SWJ6a1BRLQ==?=", b"=?ks_c_5601-1987?B?" + base64.b64encode("한국".encode("euc_kr")) + b"?=",
        b"=?TIS-620?B?" + base64.b64encode("ไทย".encode("tis_620")) + b"?=", b"=?ISO-2022-JP?B?" + base64.b64encode("髙﨑".encode("cp932").decode("cp932").encode("iso2022_jp_ext", "replace")) + b"?=",
    ]
    for i, s in enumerate(subjects, 1):
        hdr = b"Subject: " + s + b"\nFrom: " + b("ISO-2022-JP", "山田", "iso2022_jp") + b" <yamada@example.com>\n"
        if i % 3 == 0: hdr = b"Message-ID: <marked@x>\nDate-Warning: SPOOF\nToken: tok\n" + hdr + b"Date: Mon, 1 Jan 2024\nTo: someone\n"
        if i % 4 == 0: hdr += b"X-Mew-Ref: " + s + b"\n"
        open(os.path.join(mail, str(i)), "wb").write(hdr + b"\nbody " + s + b"\n")


def make_uni(mail):
    """uni: Unicode matching."""
    mail = os.path.join(mail, "uni")
    os.makedirs(mail, exist_ok=True)
    SUBJECTS = [
        "Straße", "STRASSE", "strasse", "Ärger", "Ärger", "ÄRGER", "ärger",
        "ПРИВЕТ мир", "привет", "ΟΔΥΣΣΕΥΣ", "οδυσσευς", "Οδυσσεύς", "ς σ Σ",
        "İstanbul", "ISTANBUL", "ıi", "ﬁle", "FILE", "Kkelvin", "Kelvin", "Ω ohm Ω",
        "ＡＢＣ", "abc", "ｱｲｳ", "アイウ", "日本語の件名", "ﾃｽﾄ",
        "각", "각", "ȩ́", "ȩ́", "ḉ",
        "ǅ dž Ǆ", "ŉ", "ﬀ ﬃ", "Σίσυφος", "ὈΔΥΣΣΕΎΣ", "Ⅻ ⅻ", "🄰 a", "x‍y", "x️y",
    ]
    n = [0]
    def put(subject_bytes):
        n[0] += 1
        i = n[0]
        open(os.path.join(mail, str(i)), "wb").write(b"Subject: " + subject_bytes + b"\nFrom: x@example.com\n\nbody\n")
    for s in SUBJECTS:
        put(s.encode())
        put(b"=?UTF-8?B?" + base64.b64encode(s.encode()) + b"?=")
    put(b"Stra\xdfe latin1 raw")
    put(b"\xff\xfe broken \xc3")
    put(b"=?ISO-8859-1?Q?Stra=DFe?=")
    put(b"=?ISO-8859-1?Q?=C4rger?=")


def utf7(code_points, imap=False):
    """A base64 run of UTF-7 for code points but surrogates."""
    raw = "".join(map(chr, code_points)).encode("utf-16-be")
    run = base64.b64encode(raw).rstrip(b"=")
    return run.replace(b"/", b",") if imap else run


def make_utf7(mail):
    """utf7: encoded-words in UTF-7."""
    mail = os.path.join(mail, "utf7")
    os.makedirs(mail, exist_ok=True)
    texts = [
        ("utf-7", b"+ZeVnLIqe-"), ("UTF-7", b"+ZeVnLIqe"), ("utf7", b"Stra+AN8-e"), ("Utf-7", b"caf+AOk- au lait"),
        ("utf-7", b"+ZeV-"), ("utf-7", b"a~b"), ("utf-7", b"a\\b"), ("utf-7", b"+"), ("utf-7", b"a+"),
        ("utf-7", b"+-"), ("utf-7", b"+!-"), ("utf-7", b"+2D3cAA-"), ("utf-7", b"+2D0-"), ("utf-7", b"+2D0AYQ-"),
        ("utf-7", b"+AGE~"), ("utf-7", b"x+AGE.y"), ("utf-7", b"+Ze"), ("utf-7", b"+AGEAYgBj-"),
        ("utf-7", b"+ZeV-x+ZeVnLIqe-"), ("utf-7", b"\xe9+AGE-"), ("utf-7", b"+AAA-"), ("utf-7", b"+AGE +AGI-"),
        ("UTF-7-IMAP", b"&ZeVnLIqe-"), ("utf-7-imap", b"&ZeVnLIqe"), ("utf-7-imap", b"&AGE."),
        ("utf-7-imap", b"a+b&-c"), ("utf-7-imap", b"&AGE~-"), ("x-utf-7", b"+ZeVnLIqe-"),
        ("unicode-1-1-utf-7", b"+ZeVnLIqe-"),
    ]
    rnd = random.Random(2152)
    pool = [range(0x20, 0x7f), range(0xa0, 0x800), range(0x3040, 0x30ff), range(0x4e00, 0x9fff),
            range(0xac00, 0xd7a3), range(0xe000, 0xfffe), range(0x10000, 0x10ffff)]
    for _ in range(40):
        imap = rnd.random() < 0.25
        shift = b"&" if imap else b"+"
        parts = []
        for _ in range(rnd.randint(1, 4)):
            if rnd.random() < 0.6:
                cps = [rnd.choice(rnd.choice(pool)) for _ in range(rnd.randint(1, 5))]
                parts.append(shift + utf7(cps, imap) + rnd.choice([b"-", b"-", b"", b" "]))
            else:
                parts.append(bytes(rnd.choices(b"abcXYZ019 '(),.:?!*", k=rnd.randint(1, 6))))
        texts.append(("UTF-7-IMAP" if imap else "UTF-7", b"".join(parts)))
    for i, (charset, text) in enumerate(texts, 1):
        b = b"=?" + charset.encode() + b"?B?" + base64.b64encode(text) + b"?="
        q = b"=?" + charset.encode() + b"?Q?" + b"".join(
            b"=%02X" % c if c > 126 or c < 33 or c in b"=?_" else bytes([c]) for c in text) + b"?="
        w(f"{mail}/{i}", b"Subject: " + b + b"\nX-Q: " + q + b"\nFrom: x@example.com\n\nbody\n")


def make_lists(cwd):
    """The lists for -i. list2 has a long line and a line with NUL."""
    os.makedirs(cwd, exist_ok=True)
    w(f"{cwd}/list1", b"1\n2.mew\n  3\n05\n13\n12\n10\nnonexist\n18\n26\n16.mew\nCD: +rand\n1\n2\n3\nCD: ~/Mail/a/b\n3\nCD: +nonexist\n1\n")
    w(f"{cwd}/list2", b"1\n" + b"x" * 5000 + b"\n2\n1\x00junk\n3\n4")
    w(f"{cwd}/list3", b"1\n2\n")


def main():
    if len(sys.argv) != 2:
        sys.exit("usage: mkcorpus.py DIR")
    root = sys.argv[1]
    mail = os.path.join(root, "home", "Mail")
    make_base(mail)
    make_mime(mail)
    make_uni(mail)
    make_utf7(mail)
    make_lists(os.path.join(root, "cwd"))
    w(os.path.join(root, "VERSION"), b"%d\n" % VERSION)


if __name__ == "__main__":
    main()
