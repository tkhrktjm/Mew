#!/usr/bin/env python3
"""Differential test of two mewl binaries.

Runs mewl A and mewl B with the same arguments and input over a corpus
made by mkcorpus.py, and compares the exit status, stdout, and stderr
of each run.

  difftest.py [options] A B

Each binary runs through a symbolic link named --name-a or --name-b
("mewl" by default), since mewl-rs acts by the name it is invoked as.
The name of the program in messages ("mewl: ...") and the path of the
corpus are replaced with PROG and $HOME before comparing, so that runs
by different names or on different variants of the corpus compare.
The version printed by -v is replaced with VERSION, as mewl-rs has
its own.

Suites (--suite, all but wellformed by default):

  base       options, folders and ranges, numbers around INT_MAX and
             UINT_MAX, scan, sort (-d), -i, -e and its failures, -w,
             a FIFO, errors, strings of patterns around BUFSIZ,
             patterns in hexadecimal (-P), and 3000 fuzzed pick
             patterns
  mime       pick and sort on encoded-words (folder +mime)
  uni        pick with Unicode case folding and NFC (folder +uni)
  utf7       pick and sort on encoded-words in UTF-7 (folder +utf7)
  iso2022jp  pick and sort on encoded-words in ISO-2022-JP and its
             variants, and on raw ISO-2022-JP (folder +iso2022jp)
  charset    pick on encoded-words in many charset names, to see which
             names are known, and on texts in single-byte charsets and
             in Shift_JIS and EUC-JP (folder +charset)
  wellformed 3000 well-formed pick patterns on an empty folder, to see
             that two parsers accept and refuse the same ones

Variants of the corpus (--corpus-a, --corpus-b):

  orig   as made by mkcorpus.py
  lf     a newline added to every message not ending with one
  lfdot  lf, and a line added after a last line beginning with "."
  clean  empty and headerless messages removed, and every message
         ending its header with an empty line

The mismatches can be written to a file (--dump) for classify.py.
"""

import argparse
import json
import os
import random
import re
import shutil
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))

SUITES = ("base", "mime", "uni", "utf7", "iso2022jp", "charset", "wellformed")


def make_variant(corpus, work, kind):
    """The corpus of the variant `kind`, made from `corpus` if needed."""
    if kind == "orig":
        return corpus
    dst = os.path.join(work, "corpus-" + kind)
    if os.path.exists(dst):
        return dst
    shutil.copytree(corpus, dst, symlinks=True)
    for root, _, files in os.walk(os.path.join(dst, "home", "Mail")):
        for fn in files:
            path = os.path.join(root, fn)
            if os.path.islink(path):
                continue
            with open(path, "rb") as f:
                data = f.read()
            if kind == "clean":
                if not data or data.startswith(b"\n"):
                    os.remove(path)
                    continue
                if not data.endswith(b"\n"):
                    data += b"\n"
                if b"\n\n" not in data:
                    data += b"\n"
            else:
                if not data or len(data) >= 65536:
                    continue
                if not data.endswith(b"\n"):
                    data += b"\n"
                if kind == "lfdot" and data.rstrip(b"\n").split(b"\n")[-1].startswith(b"."):
                    data += b"end\n"
            with open(path, "wb") as f:
                f.write(data)
    return dst


def corpus_version(corpus):
    try:
        with open(os.path.join(corpus, "VERSION")) as f:
            return int(f.read())
    except (OSError, ValueError):
        return None


def mkcorpus_version():
    sys.path.insert(0, HERE)
    import mkcorpus
    return mkcorpus.VERSION


class Runner:
    def __init__(self, args):
        work = os.path.abspath(args.work)
        os.makedirs(work, exist_ok=True)
        corpus = os.path.join(work, "corpus")
        if corpus_version(corpus) != mkcorpus_version():
            for name in os.listdir(work):
                if name == "corpus" or name.startswith("corpus-"):
                    shutil.rmtree(os.path.join(work, name))
            subprocess.run([sys.executable, os.path.join(HERE, "mkcorpus.py"), corpus], check=True)
        self.home = os.path.join(corpus, "home")
        self.cwd = os.path.join(corpus, "cwd")
        self.side = {}
        for d, binary, name, kind in (("a", args.a, args.name_a, args.corpus_a),
                                      ("b", args.b, args.name_b, args.corpus_b)):
            bindir = os.path.join(work, "bin-" + d)
            shutil.rmtree(bindir, ignore_errors=True)
            os.makedirs(bindir)
            link = os.path.join(bindir, name)
            os.symlink(os.path.abspath(binary), link)
            self.side[d] = (link, os.path.join(make_variant(corpus, work, kind), "home"))
        self.env = dict(os.environ, PATH=HERE + os.pathsep + os.environ["PATH"])
        self.dump = args.dump
        self.show = args.show
        self.total = 0
        self.mismatches = 0

    def run(self, d, args, stdin, timeout):
        link, home = self.side[d]
        args = [x.replace(self.home, home) for x in args]
        try:
            p = subprocess.run([link] + args, input=stdin, capture_output=True, cwd=self.cwd,
                               env=dict(self.env, HOME=home), timeout=timeout)
        except subprocess.TimeoutExpired:
            return ("timeout", b"", b"")
        norm = lambda b: b.replace(home.encode(), b"$HOME")
        name = os.path.basename(link).encode()
        err = re.sub(rb"(?m)^(PROG version) .*", rb"\1 VERSION", norm(p.stderr).replace(name, b"PROG"))
        return (p.returncode, norm(p.stdout), err)

    def check(self, args, stdin=b"", timeout=30):
        self.total += 1
        a = self.run("a", args, stdin, timeout)
        b = self.run("b", args, stdin, timeout)
        if a == b:
            return
        self.mismatches += 1
        if self.dump:
            latin1 = lambda r: [r[0], r[1].decode("latin1"), r[2].decode("latin1")]
            record = {"args": [x.encode("utf-8", "surrogateescape").decode("latin1") for x in args],
                      "a": latin1(a), "b": latin1(b)}
            with open(self.dump, "a") as f:
                f.write(json.dumps(record) + "\n")
        if self.mismatches <= self.show:
            print("MISMATCH:", args)
            for k, (x, y) in enumerate(zip(a, b)):
                if x != y:
                    print("  " + ("exit", "stdout", "stderr")[k], "\n   A:", repr(x)[:400], "\n   B:", repr(y)[:400])


def suite_base(r):
    home = r.home
    folders = [[], ["+inbox"], ["+rand"], ["+my\x07folder"], ["+a/b"], ["+empty"], ["+nonexist"],
               [f"{home}/Mail/inbox"], ["~/Mail/inbox"], ["~"], ["~x"], ["+"], [""], ["-"], ["+inbox 1-5"],
               ["+inbox", "3"], ["+inbox", "last:3"], ["+inbox", "-3"], ["+inbox", "10-"], ["+inbox", "2-12"],
               ["+inbox", "last:-3"], ["+inbox", "0"], ["+inbox", "abc"], ["+rand", "last:200"], ["a", "b", "c"]]
    optsets = [[], ["-l", "0"], ["-l", "2"], ["-l", "-1"], ["-c", "0"], ["-c", "3"], ["-c", "-1"],
               ["-f", "Subject:,From:"], ["-f", "References:,Subject:"], ["-f", ""], ["-f", "Subject:,,From:"],
               ["-f", "subject,References:,References:"], ["-x", ""], ["-x", ".txt"], ["-x", ".mew.mew"], ["-n"],
               ["-b", "/nonexist"], ["-b", "~"], ["-b", "~/Mail"], ["-b", "~x"], ["-b", f"{home}/Mail"], ["-b", ""],
               ["-w"]]
    for f in folders:
        for o in optsets:
            r.check(o + f, b"\n" * 300)
    for o in optsets:
        r.check(o + ["-s", "+inbox 1-3", "-s", "+rand last:5"], b"\n" * 300)
    for key in ["Subject:", "subject", "From:", "References:", "X-None:", "", "content-type", "Subj"]:
        for f in folders[:6] + [["+inbox", "1-3"]]:
            for o in [[], ["-l", "0"], ["-l", "1"], ["-f", "Subject:"]]:
                r.check(o + ["-d", key] + f)
    for args in [["-h"], ["-v"], ["-z"], ["--"], ["-ab"], ["-:"], ["-b"], ["-s", "+inbox", "+rand"], ["-s"],
                 sum([["-s", "+inbox"]] * 17, []), sum([["-s", "+inbox 1"]] * 16, []), ["+inbox", "-a"],
                 ["-a", "-a", "-p", "subject=hello", "+inbox"], ["-i", "nonexist"], ["-e", "nonexist-cmd", "+inbox"],
                 ["-w", "-w", "+inbox"]]:
        r.check(args, b"\n" * 300)
    with open(os.path.join(r.cwd, "list1"), "rb") as f:
        list1 = f.read()
    for lst in ["list1", "list2", "list3", "-"]:
        for f in [[], ["+inbox"], ["+nonexist"], ["+my\x07folder"], ["-s", "+rand", "-s", "+inbox"]]:
            for o in [[], ["-x", ""], ["-d", "Subject:"], ["-p", "subject=e"], ["-a", "-p", "subject=e"], ["-w"]]:
                r.check(o + ["-i", lst] + f, list1)
    for f in [["+inbox"], [], ["+nonexist"]]:
        for o in [[], ["-m", "a b  c"], ["-m", ""], ["-m", " ".join(str(i) for i in range(20))], ["-w"],
                  ["-p", "head=foo"]]:
            r.check(o + ["-e", "fake-inc"] + f, b"\n" * 100)
            r.check(o + ["-e", os.path.join(HERE, "fake-inc")] + f, b"\n" * 100)
    # the command of -e failing: exiting with a status but 0, or killed
    for how in ["0", "3", "kill"]:
        for o in [[], ["-d", "subject:"], ["-p", "subject=e"]]:
            r.check(o + ["-e", "fake-fail", "-m", how, "+inbox"])
    r.check(["-w", "+inbox"], b"\n\n")
    r.check(["-w", "+inbox"], b"")
    # patterns in hexadecimal (-P), and where they end
    hexes = [p.encode().hex() for p in ["subject=hello", "subject=日本語", "head=foo | from=bar", "(subject=e",
                                        "subject=e\0x", "subject=e from=x", "a!="]]
    hexes += ["7375626A6563743D454C4C4F", "", "6", "616", "6g", "g6", "+6", " 61", "-p"]
    for h in hexes:
        for f in [["+inbox"], ["+rand"]]:
            r.check(["-P", h] + f)
    r.check(["-p", "subject=x", "-P", "subject=e".encode().hex(), "+inbox"])
    r.check(["-P", "subject=x".encode().hex(), "-p", "subject=e", "+inbox"])
    r.check(["-P"])
    # head with a newline, which an empty header has, and "!=" at the end
    for p in ["head=\n", "head!=\n", 'head="\n"', "head==\n", "a!=", "subject!=", "a!", "a!=="]:
        for f in [["+inbox"], ["+rand"]]:
            r.check(["-p", p] + f)
    # numbers around INT_MAX and UINT_MAX, negative ones, and garbage
    for f in [["+big"], ["+big", "2147483648"], ["+big", "4294967295"], ["+big", "4294967296"],
              ["+big", "99999999999"], ["+big", "2-4294967295"], ["+big", "2-4294967296"], ["+big", "4294967296-"],
              ["+big", "last:2"], ["+big", "last:4294967296"], ["+big", "last: -2"], ["+big", "1--3"],
              ["+big", "x-2147483648y"], ["+big", "+2147483648"], ["-s", "+big last:-1"]]:
        r.check(f)
    for o in [["-l", "4294967295"], ["-l", "4294967296"], ["-l", "3000000000"], ["-c", "99999999999"],
              ["-c", " -1"], ["-c", "+2"], ["-c", "2x"], ["-l", "-1", "-z"]]:
        r.check(o + ["+big", "1"])
    # strings of a pattern around BUFSIZ of glibc
    for n in [8190, 8191, 8192]:
        for p in [f"subject={'x' * n}", f'subject="{"x" * n}"', f"subject=\\{'x' * n}", f"{'x' * n}=a"]:
            r.check(["-p", p, "+inbox"])
    # a FIFO among messages, made here as copying the corpus fails on it
    fifos = sorted({os.path.join(home, "Mail", "fifo", "2") for _, home in r.side.values()})
    for fifo in fifos:
        if not os.path.exists(fifo):
            os.mkfifo(fifo)
    try:
        for o in [[], ["-n"], ["-d", "subject:"], ["-n", "-d", "subject:"], ["-p", "subject=e"]]:
            r.check(o + ["+fifo"], timeout=5)
        r.check(["-i", "-", "+fifo"], b"1\n2\n3\n", timeout=5)
        r.check(["-n", "-i", "-", "+fifo"], b"1\n2\n3\n", timeout=5)
    finally:
        for fifo in fifos:
            os.remove(fifo)


def suite_mime(r):
    values = ["日本語", "日本", "件名", "語", "の日本", "日本語の日本", "café", "CAFÉ", "Résumé", "résumé", "中文",
              "한국", "Привет", "山田", "〜", "～", "①", "髙", "ไทย", "uro", "€uro", "plain ascii", "hi", "abc",
              "a\tb", "ac", "a b", "Re:", "Re: 日本語", "text", "raw jis", "\udcff", "=?", "?=", "4", "の4", "tok",
              "SPOOF", "marked", ""]
    keys = ["subject", "Subject:", "subject:", "from", "From:", "message", "message-id", "x-mew", "x-mew-ref",
            "date", "to", "token", "head", "Subj", "s"]
    for k in keys:
        for v in values:
            for op in ["=", "=="]:
                r.check(["-p", f'{k}{op}"{v}"', "+mime"])
    for key in ["subject", "Subject:", "from", "message", "Message-ID:", "date", "Date:", "to", "x-mew", "token",
                "X-Mew-Ref:"]:
        for o in [[], ["-l", "0"]]:
            r.check(o + ["-d", key, "+mime"])
    for o in [[], ["-a"], ["-l", "0", "-a"]]:
        r.check(o + ["-p", "subject=e | from=a", "+mime"])
    r.check(["+mime"])
    r.check(["-l", "0", "+mime"])


UNI = ["Straße", "STRASSE", "strasse", "ss", "SS", "ß", "ẞ", "Ärger", "Ärger", "ärger", "ä", "̈",
       "привет", "ПРИВЕТ", "οδυσσευς", "ΟΔΥΣΣΕΥΣ", "σ", "ς", "Σ", "istanbul", "İstanbul", "i̇", "ı", "I",
       "file", "ﬁ", "FI", "kelvin", "K", "k", "ω", "Ω", "Ω", "abc", "ＡＢＣ", "ａｂｃ", "アイウ", "ｱｲｳ",
       "日本語", "件名", "가", "가", "각", "ȩ́", "ȩ́", "ḉ", "ç",
       "ǆ", "Ǆ", "dž", "ʼn", "ff", "ffi", "σίσυφος", "ὀδυσσεύς", "ⅻ", "Ⅻ", "xii", "🄰", "xy", "x‍y", "️",
       "\udcdf", "\udcff"]


def suite_uni(r):
    for k in ["subject", "Subject:", "head", "from"]:
        for v in UNI:
            for op in ["=", "==", "!=", "!=="]:
                r.check(["-p", f'{k}{op}"{v}"', "+uni"])
    rnd = random.Random(7)
    for _ in range(600):
        v = "".join(rnd.choices(UNI, k=rnd.randint(1, 3)))
        op = rnd.choice(["=", "=="])
        r.check(["-p", f'subject{op}"{v}"', "+uni"])
    r.check(["+uni"])
    r.check(["-d", "subject:", "+uni"])


def suite_utf7(r):
    values = ["日本語", "日本", "日", "Straße", "STRASSE", "café", "CAFÉ", "au lait", "a~b", "a\\b", "+", "-", "!-", "a",
              "abc", "a b", "AYQ", "~", "x", "\U0001f400", "日-", "+ZeVnLIqe", "ZeV", "&", "a+b&c", ".", "\udce9", ""]
    for k in ["subject", "x-q", "head"]:
        for v in values:
            for op in ["=", "=="]:
                r.check(["-p", f'{k}{op}"{v}"', "+utf7"])
    rnd = random.Random(7)
    chars = "".join(chr(c) for c in range(0x20, 0x7f)) + "日本語のテスト中文한국어éàüß\U0001f400"
    for _ in range(300):
        v = "".join(rnd.choices(chars, k=rnd.randint(1, 3))).replace("\\", "").replace('"', "")
        r.check(["-p", f'{rnd.choice(["subject", "x-q"])}{rnd.choice(["=", "=="])}"{v}"', "+utf7"])
    r.check(["+utf7"])
    r.check(["-d", "subject:", "+utf7"])
    r.check(["-d", "x-q", "+utf7"])


def suite_iso2022jp(r):
    values = ["日本語", "日本", "日", "Re: 日本 (1)", "¥‾", "\\~", "〜", "～", "‖", "∥", "①", "-!", "Gojō", "Goj", "$(D+W",
              "中文", "・", "·", "한국", "㉾", "ｱｲ", "café", "α", "β", "F|", "\udcff", ".!", "日 本", "a", "b", ""]
    for k in ["subject", "x-q", "user-agent", "head"]:
        for v in values:
            for op in ["=", "=="]:
                r.check(["-p", f'{k}{op}"{v}"', "+iso2022jp"])
    r.check(["+iso2022jp"])
    for k in ["subject:", "x-q", "user-agent"]:
        r.check(["-d", k, "+iso2022jp"])


# Codecs of Python to guess the characters of mkcorpus.CHARSET_TEXTS.
GUESS_CODECS = ["latin-1", "cp1252", "iso8859_9", "cp1254", "iso8859_11", "tis_620", "cp874", "cp1250", "cp1251",
                "cp1253", "cp1256", "cp1257", "koi8_r", "koi8_u", "mac_roman", "cp866", "iso8859_15", "iso8859_8",
                "iso8859_6"]


def suite_charset(r):
    import mkcorpus
    for v in ["mewl", "MEWL", "mew", "=?"]:
        for op in ["=", "==", "!="]:
            r.check(["-p", f'subject{op}"{v}"', "+charset"])
    chars = set()
    for _, text in mkcorpus.CHARSET_TEXTS:
        for codec in GUESS_CODECS:
            chars |= set(text.decode(codec, "ignore"))
    chars |= set("日本〜～‖∥−－¢￠£￡¬￢—―¥‾~①ⅰ￤¦ｱ\ue000\ue3ac\x85")
    for c in sorted(chars - set('"\\')):
        r.check(["-p", f'subject=="{c}"', "+charset"])
    r.check(["-d", "subject:", "+charset"])


TOKENS = ["subject", "from", "head", "references", "to", "x-foo", "=", "==", "!=", "!==", "!", "&", "&&", "|", "||",
          "(", ")", "foo", "FOO", "bar", "\"a b\"", "\\(", "\"", "\\", " ", "<id@x>", "あ", "\udce3", "\udcff",
          "\udc81x", "xあ", ".", "", "=foo", "subject=", "x", "\t"]


def well_formed(rnd, depth=0):
    """A pick pattern built by the grammar; with \\udcXX for raw bytes."""
    x = rnd.random()
    if depth > 3 or x < 0.4:
        return (rnd.choice(["subject", "from", "head", "to", "references", "x-foo", "Subject:", "cc"])
                + rnd.choice(["=", "==", "!=", "!=="])
                + rnd.choice(["foo", "FOO", "bar", "\"a b\"", "e", "\"\"", "<id@x>", "x", ".", "あ", "xあ", "\udcff",
                              "high", "\udce3\udc81\udc82", "\udc80"]))
    if x < 0.55:
        return "!" + well_formed(rnd, depth + 1)
    if x < 0.7:
        return "(" + well_formed(rnd, depth + 1) + ")"
    return well_formed(rnd, depth + 1) + rnd.choice(["&", "|", " & ", "&&", "||", " | "]) + well_formed(rnd, depth + 1)


def suite_fuzz(r, seed):
    """Half well-formed, half random sequences of tokens."""
    rnd = random.Random(seed)
    for i in range(1500):
        pat = well_formed(rnd) if i % 2 else "".join(rnd.choices(TOKENS, k=rnd.randint(0, 8)))
        for f in [["+inbox"], ["+rand"]]:
            o = rnd.choice([[], ["-a"], ["-a", "-l", "1"], ["-l", "0"], ["-a", "-c", "2", "-w"]])
            r.check(o + ["-p", pat] + f, b"\n" * 300)


def suite_wellformed(r):
    for seed in range(3):
        rnd = random.Random(seed)
        for _ in range(1000):
            r.check(["-p", well_formed(rnd), "+empty"])


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("a", help="mewl A")
    p.add_argument("b", help="mewl B")
    p.add_argument("--name-a", default="mewl", help="the name A is invoked as")
    p.add_argument("--name-b", default="mewl", help="the name B is invoked as")
    p.add_argument("--corpus-a", default="orig", choices=("orig", "lf", "lfdot", "clean"))
    p.add_argument("--corpus-b", default="orig", choices=("orig", "lf", "lfdot", "clean"))
    p.add_argument("--suite", default="base,mime,uni,utf7,iso2022jp,charset", help="comma-separated: " + ", ".join(SUITES))
    p.add_argument("--seed", type=int, default=1, help="seed of the fuzzed patterns in base")
    p.add_argument("--work", default=os.path.join(HERE, "work"), help="where the corpus is made")
    p.add_argument("--dump", help="append the mismatches to this file as JSON lines")
    p.add_argument("--show", type=int, default=10, help="how many mismatches to print")
    args = p.parse_args()
    suites = args.suite.split(",")
    for s in suites:
        if s not in SUITES:
            p.error(f"unknown suite: {s}")

    r = Runner(args)
    for name, fn in (("base", suite_base), ("mime", suite_mime), ("uni", suite_uni), ("utf7", suite_utf7),
                     ("iso2022jp", suite_iso2022jp), ("charset", suite_charset),
                     ("wellformed", suite_wellformed)):
        if name in suites:
            before = (r.total, r.mismatches)
            fn(r)
            print(f"{name}: {r.total - before[0]} run, {r.mismatches - before[1]} mismatches")
    if "base" in suites:
        before = (r.total, r.mismatches)
        suite_fuzz(r, args.seed)
        print(f"base (fuzzed patterns): {r.total - before[0]} run, {r.mismatches - before[1]} mismatches")
    print(f"TOTAL: {r.total} run, {r.mismatches} mismatches")


if __name__ == "__main__":
    main()
