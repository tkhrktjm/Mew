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
  iso2022jp   encoded-words in ISO-2022-JP and its variants, and raw
              ISO-2022-JP in a header
  charset     an encoded-word of "mewl" in each of many charset names:
              those of the Encoding Standard, of glibc, of neither, and
              their spellings; and texts in single-byte charsets

DIR/cwd gets list1, list2 and list3, the lists for -i, and is where mewl
runs.
"""

import base64
import os
import random
import sys

# Bump this when the corpus changes, so that difftest.py makes it again.
VERSION = 5


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


def make_iso2022jp(mail):
    """iso2022jp: encoded-words in ISO-2022-JP and its variants, and raw
    ISO-2022-JP in a header."""
    mail = os.path.join(mail, "iso2022jp")
    os.makedirs(mail, exist_ok=True)
    esc = {"ascii": b"\x1b(B", "roman": b"\x1b(J", "kana": b"\x1b(I", "jis78": b"\x1b$@", "jis": b"\x1b$B",
           "gb": b"\x1b$A", "ksc": b"\x1b$(C", "x0212": b"\x1b$(D", "latin": b"\x1b.A", "greek": b"\x1b.F",
           "ss2": b"\x1bN", "x0213": b"\x1b$(Q"}
    a = esc["ascii"]
    texts = [
        ("ISO-2022-JP", esc["jis"] + b"F|K\\8l" + a), ("iso-2022-jp", b"Re: " + esc["jis"] + b"F|K\\" + a + b" (1)"),
        ("ISO-2022-JP", esc["jis78"] + b"F|" + esc["roman"] + b"\\~" + a + b"\\~"),
        ("ISO-2022-JP", esc["jis"] + b"!A!B!]!q!r\"L" + a), ("ISO-2022-JP", esc["jis"] + b"-!-\"-5" + a),
        ("ISO-2022-JP", esc["jis"] + b"y!|q" + a), ("ISO-2022-JP", b"Goj" + esc["x0212"] + b"+W" + a),
        ("ISO-2022-JP", esc["kana"] + b"12" + a), ("ISO-2022-JP", esc["jis"] + b"F|\xffK\\" + a),
        ("ISO-2022-JP", esc["jis"] + b"F|K"), ("ISO-2022-JP", b"a" + esc["jis"][:2]), ("ISO-2022-JP", b"a\x1bZb"),
        ("ISO-2022-JP", esc["jis"] + b"F| K\\\tF|" + a), ("ISO-2022-JP-1", b"Goj" + esc["x0212"] + b"+W" + a),
        ("ISO-2022-JP-2", b"Goj" + esc["x0212"] + b"+W" + a), ("iso-2022-jp-2", esc["gb"] + b"VPND!$!*" + a),
        ("ISO-2022-JP-2", esc["ksc"] + b"GQ19\"h" + a), ("ISO-2022-JP-2", esc["gb"] + b"*!x!" + a),
        ("ISO-2022-JP-2", esc["latin"] + b"caf" + esc["ss2"] + b"i " + esc["greek"] + esc["ss2"] + b"a" + esc["ss2"] + b"2"),
        ("ISO-2022-JP-2", esc["ss2"] + b"a"), ("ISO-2022-JP-2", esc["kana"] + b"1`" + a),
        ("ISO-2022-JP-2", esc["jis"] + b"-!" + a), ("ISO-2022-JP-3", esc["jis"] + b"F|" + esc["x0213"] + b".!" + a),
        ("csISO2022JP", esc["jis"] + b"F|" + a), ("ISO2022JP2", esc["x0212"] + b"+W" + a),
    ]
    for i, (charset, text) in enumerate(texts, 1):
        b = b"=?" + charset.encode() + b"?B?" + base64.b64encode(text) + b"?="
        q = b"=?" + charset.encode() + b"?Q?" + b"".join(
            b"=%02X" % c if c > 126 or c < 33 or c in b"=?_" else bytes([c]) for c in text) + b"?="
        w(f"{mail}/{i}", b"Subject: " + b + b"\nX-Q: " + q + b"\nFrom: x@example.com\n\nbody\n")
    raw = [b"FLIM/1.14.9 (Goj" + esc["x0212"] + b"+W" + a + b")", b"a " + esc["jis"] + b"F|K\\" + a + b" b",
           esc["gb"] + b"VPND" + a, esc["jis"] + b"-!" + a, b"x " + esc["x0213"] + b".!" + a + b" y"]
    for i, text in enumerate(raw, len(texts) + 1):
        w(f"{mail}/{i}", b"Subject: raw\nUser-Agent: " + text + b"\nFrom: x@example.com\n\nbody\n")


# Charset names: of the Encoding Standard but not of glibc, of glibc but
# not of the Encoding Standard, of both, of neither, renamed by mewl, and
# spelled as glibc normalizes them.
CHARSET_NAMES = """
csbig5 x-x-big5 x-euc-jp csksc56011987 iso-ir-149 korean ks_c_5601-1987 ks_c_5601-1989 ksc5601 ksc_5601
windows-949 chinese csiso58gb231280 gb_2312 gb_2312-80 iso-ir-58 x-gbk csisolatin9 l9 csiso88596e
csiso88596i iso-8859-6-e iso-8859-6-i sun_eu_greek csiso88598i iso-8859-8-i logical csiso88598e
iso-8859-8-e visual koi koi8_r x-sjis unicodefffe iso-10646-ucs-2 unicodefeff unicode-1-1-utf-8
unicode11utf8 unicode20utf8 x-unicode20utf8 x-mac-roman x-cp1250 x-cp1252 x-cp1258 dos-874
x-mac-cyrillic x-mac-ukrainian x-user-defined hz-gb-2312 replacement
us-ascii US-ASCII ascii us ANSI_X3.4-1968 iso646-us csascii iso-8859-1 ISO-8859-1 latin1 l1 cp819
iso_8859-1 iso8859-1 iso88591 iso-8859-2 iso-8859-3 iso-8859-4 iso-8859-5 iso-8859-6 iso-8859-7
iso-8859-8 iso-8859-9 iso-8859-10 iso-8859-11 iso-8859-13 iso-8859-14 iso-8859-15 iso-8859-16 latin9
latin5 greek cyrillic arabic hebrew windows-1250 windows-1251 windows-1252 windows-1253 windows-1254
windows-1255 windows-1256 windows-1257 windows-1258 cp1250 cp1252 cp1258 ms-ansi utf-8 UTF-8 utf8
utf-16 UTF-16 utf-16le utf-16be utf16 unicode ucs-2 ucs-2le utf-32 ucs-4 shift_jis Shift_JIS sjis
ms_kanji csshiftjis cp932 ms932 windows-31j euc-jp EUC-JP eucjp ujis euc-jp-ms eucjp-ms iso-2022-jp
ISO-2022-JP iso-2022-jp-1 iso-2022-jp-2 iso-2022-jp-3 csiso2022jp iso2022jp gb2312 gbk gb18030 cp936
euc-cn big5 big5-hkscs cp950 euc-kr cp949 uhc johab koi8-r koi8-u koi8-ru koi-8 ibm866 cp866 866
macintosh mac tis-620 tis620 cp874 windows-874 ibm874 utf-7 utf7 utf-7-imap x-utf-7
unicode-1-1-utf-7 iso-2022-kr iso-2022-cn ibm037 ebcdic-us x-unknown unknown-8bit unknown x
utf~8 s!jis SHIFT-JIS ISO-8859_1 latin-1 utf_8 euc_jp Shift_JIS*ja
""".split()


# Texts in single-byte charsets, with bytes where glibc and the
# Encoding Standard differ, and bytes invalid in either.
CHARSET_TEXTS = [
    ("iso-8859-1", b"caf\xe9 \x80\x9f\xa4\xff"), ("latin1", b"\x85\x8a\x9c"), ("us-ascii", b"caf\xe9"),
    ("ascii", b"a\x80\xff"), ("iso-8859-9", b"\xd0\xdd\xde\xf0\xfd\xfe\x80\x8a"),
    ("iso-8859-11", b"\xa1\x80\x85\x91\xdb"), ("tis-620", b"\xa1\x80\x85\xa0"), ("TIS620", b"\xa1\x80\xa0"),
    ("cp874", b"\x80\x81\xa1\xdb"), ("windows-874", b"\x80\x81\x91\xa0"), ("windows-1252", b"\x80\x81\x8d\x9f"),
    ("windows-1250", b"\x80\x81\x83\x8a"), ("windows-1251", b"\x98\x88\xc0"), ("windows-1253", b"\x81\xaa\xd2\xa2"),
    ("windows-1254", b"\x80\x8e\xd0\x9e"), ("windows-1256", b"\x81\x90\xc7"), ("windows-1257", b"\x80\xa1\xa5\x8d"),
    ("koi8-r", b"\xc1\xa3\x9a"), ("koi8-u", b"\xae\xbe\xa4"), ("koi8-ru", b"\x93\x96\xae\xbe"),
    ("koi-8", b"\x80\xc1\xff\xa3"), ("macintosh", b"\xc6\xf0\xdb\x8e"), ("ibm866", b"\x80\xf0\xff"),
    ("latin9", b"\xa4\xbd\xe9"), ("iso-8859-8-i", b"\xe0\xfd\xa1"), ("iso-8859-6", b"\xc7\xa1\xbf"),
]


def make_charset(mail):
    """charset: an encoded-word of "mewl" in each charset name, and texts
    in single-byte charsets."""
    mail = os.path.join(mail, "charset")
    os.makedirs(mail, exist_ok=True)
    words = [(name, b"mewl") for name in CHARSET_NAMES] + CHARSET_TEXTS
    for i, (name, text) in enumerate(words, 1):
        word = b"=?" + name.encode() + b"?B?" + base64.b64encode(text) + b"?="
        w(f"{mail}/{i}", b"Subject: " + word + b"\nX-Charset: " + name.encode() + b"\nFrom: x@example.com\n\nbody\n")


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
    make_iso2022jp(mail)
    make_charset(mail)
    make_lists(os.path.join(root, "cwd"))
    w(os.path.join(root, "VERSION"), b"%d\n" % VERSION)


if __name__ == "__main__":
    main()
