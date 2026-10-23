#!/usr/bin/env python3
"""Classify the mismatches dumped by difftest.py --dump.

Each command sorts the mismatches between an old mewl-rs (A) and a new
one (B) by the change that should explain them, and counts the rest as
"other", printing a few of them. "other" should be 0.

  classify.py strict DUMP --a A [--name-a NAME] [--corpus-a KIND]
      For "keep the last line and parse patterns strictly":
        refused       B refuses the pattern ("broken pattern.")
        joined        B joins words not quoted into the value; A run
                      with that value quoted gives what B gave (or
                      differs by "last line" only)
        last line     B escapes the last line of a body beginning
                      with "." where A did not
      Run difftest.py with --corpus-a lf on the same corpus first if
      A should keep the last line too.

  classify.py bugfix DUMP
      For "fix the bugs of mewl that need no change of Mew", on the
      clean corpus:
        invalid number/range  B refuses a number that is not one
        -d without ":"        B does not begin the value with ":"
        0xff in pattern       B takes 0xFF in a pattern as a byte
        NUL in a list         B reads the line after a line with NUL
        -h / -:               the help and "-:"

  classify.py pick DUMP
      For a change in comparing values: a pick on a field, a pick on
      the whole header (head), or anything else.
"""

import argparse
import collections
import json
import os
import re
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
from difftest import make_variant  # noqa: E402


def load(dump):
    with open(dump) as f:
        for line in f:
            d = json.loads(line)
            args = [x.encode("latin1").decode("utf-8", "surrogateescape") for x in d["args"]]
            res = lambda r: (r[0], r[1].encode("latin1"), r[2].encode("latin1"))
            yield args, res(d["a"]), res(d["b"])


def pattern_of(args):
    return args[args.index("-p") + 1].encode("utf-8", "surrogateescape") if "-p" in args else None


def report(cats, others):
    for k, v in cats.items():
        print(f"{k}: {v}")
    print(f"other: {len(others)}")
    for x in others[:10]:
        print("  ", x)


def lex(p):
    """The tokens of a pattern as mewl-rs reads them: (kind, value,
    start, end), where kind is S (string), C (comparison), or one of
    & | ! ( )."""
    toks, i, n = [], 0, len(p)
    while True:
        while i < n and p[i] in b" \t":
            i += 1
        if i >= n or p[i] == 0xff:
            return toks
        start, c = i, p[i]
        i += 1
        if c in b"()":
            toks.append((chr(c), None, start, i))
            continue
        if c == ord('"'):
            s = b""
            while i < n:
                if p[i] == ord("\\"):
                    s += p[i + 1:i + 2]
                    i += 2
                    continue
                if p[i] == ord('"'):
                    i += 1
                    break
                s += bytes([p[i]])
                i += 1
            toks.append(("S", s, start, i))
            continue
        if c in b"&|":
            if i < n and p[i] == c:
                i += 1
            elif i < n and p[i] in b" \t":
                i += 1
                toks.append((chr(c), None, start, i - 1))
                continue
            toks.append((chr(c), None, start, i))
            continue
        if c == ord("="):
            if i < n and p[i] == ord("="):
                i += 1
            toks.append(("C", None, start, i))
            continue
        if c == ord("!"):
            if i < n and p[i] == ord("="):
                i += 1
                if i < n and p[i] == ord("="):
                    i += 1
                toks.append(("C", None, start, i))
                continue
            toks.append(("!", None, start, i))
            continue
        s = b""
        i -= 1
        if p[i] == ord("\\"):
            s += p[i + 1:i + 2]
            i += 2
        else:
            s += bytes([p[i]])
            i += 1
        while i < n and p[i] not in b" \t&|!=()" and p[i] != 0xff:
            if p[i] == ord("\\"):
                s += p[i + 1:i + 2]
                i += 2
            else:
                s += bytes([p[i]])
                i += 1
        toks.append(("S", s, start, i))


def joined_pattern(p):
    """The pattern with values joined as mewl-rs does and quoted, or
    None if nothing is joined."""
    t = lex(p)
    out, i, joined = [], 0, False
    while i < len(t):
        k, v, s, e = t[i]
        if k == "S" and i > 0 and t[i - 1][0] == "C":
            while i + 1 < len(t) and t[i + 1][0] == "S" and not (i + 2 < len(t) and t[i + 2][0] == "C"):
                v = v + p[e:t[i + 1][2]] + t[i + 1][1]
                e = t[i + 1][3]
                i += 1
                joined = True
        out.append((k, v, s, e))
        i += 1
    if not joined:
        return None
    text = b""
    for k, v, s, e in out:
        if k == "S":
            text += b' "' + v.replace(b"\\", b"\\\\").replace(b'"', b'\\"') + b'"'
        else:
            text += b" " + p[s:e]
    return text


def escaped_last_line(a, b):
    """Whether the outputs differ only by lines which B escapes."""
    if a[0] != b[0] or a[2] != b[2]:
        return False
    al, bl = a[1].split(b"\n"), b[1].split(b"\n")
    return len(al) == len(bl) and all(x == y or (x.startswith(b".") and y == b"." + x) for x, y in zip(al, bl))


def cmd_strict(opts):
    work = os.path.abspath(opts.work)
    corpus = os.path.join(work, "corpus")
    home = os.path.join(make_variant(corpus, work, opts.corpus_a), "home")
    bindir = os.path.join(work, "bin-classify")
    os.makedirs(bindir, exist_ok=True)
    link = os.path.join(bindir, opts.name_a)
    if os.path.lexists(link):
        os.remove(link)
    os.symlink(os.path.abspath(opts.a), link)
    env = dict(os.environ, HOME=home, PATH=HERE + os.pathsep + os.environ["PATH"])

    def run_a(args):
        args = [x.replace(os.path.join(corpus, "home"), home) for x in args]
        p = subprocess.run([link] + args, capture_output=True, cwd=os.path.join(corpus, "cwd"), env=env,
                           input=b"\n" * 300, timeout=30)
        norm = lambda s: s.replace(home.encode(), b"$HOME")
        return (p.returncode, norm(p.stdout), norm(p.stderr).replace(opts.name_a.encode(), b"PROG"))

    cats, others = collections.Counter(), []
    for args, a, b in load(opts.dump):
        pat = pattern_of(args)
        if pat is not None and b"PROG: broken pattern." in b[2]:
            cats["refused"] += 1
            continue
        joined = joined_pattern(pat) if pat is not None else None
        if joined is not None:
            i = args.index("-p")
            quoted = args[:i + 1] + [joined.decode("utf-8", "surrogateescape")] + args[i + 2:]
            a2 = run_a(quoted)
            if a2 == b:
                cats["joined"] += 1
                continue
            if escaped_last_line(a2, b):
                cats["joined, and last line"] += 1
                continue
        if escaped_last_line(a, b):
            cats["last line"] += 1
            continue
        others.append(args)
    report(cats, others)


def valid_number(x, limit):
    return re.fullmatch(rb"\d+", x) is not None and int(x) < limit


def valid_range(r):
    if r.startswith(b"last:"):
        return valid_number(r[5:], 2 ** 32)
    if b"-" in r:
        return all(x == b"" or valid_number(x, 2 ** 32) for x in r.split(b"-", 1))
    return valid_number(r, 2 ** 32)


def cmd_bugfix(opts):
    cats, others = collections.Counter(), []
    for args, a, b in load(opts.dump):
        bargs = [x.encode("utf-8", "surrogateescape") for x in args]
        m = re.match(rb"PROG: invalid (number|range) '(.*)'\.\n$", b[2], re.S)
        if m and b[0] == 1 and b[1] == b"":
            kind, val = m.groups()
            if kind == b"number":
                ok = any(bargs[i] in (b"-c", b"-l") and bargs[i + 1] == val and not valid_number(val, 2 ** 31)
                         for i in range(len(bargs) - 1))
            else:
                ok = not valid_range(val) and any(val in x for x in bargs)
            if ok:
                cats[f"invalid {kind.decode()}"] += 1
                continue
        elif args in (["-h"], ["-:"]):
            cats["-h / -:"] += 1
            continue
        elif "-p" in args and b"\xff" in pattern_of(args):
            cats["0xff in pattern"] += 1
            continue
        elif "-d" in args and not args[args.index("-d") + 1].endswith(":"):
            fixed = re.sub(rb"(?m)^(\d+)::", rb"\1:", a[1])
            if (a[0], fixed, a[2]) == b:
                cats['-d without ":"'] += 1
                continue
        elif "-i" in args and "list2" in args:
            cats["NUL in a list"] += 1
            continue
        others.append(args)
    report(cats, others)


def cmd_pick(opts):
    cats, others = collections.Counter(), []
    for args, a, b in load(opts.dump):
        pat = pattern_of(args)
        if pat is None:
            others.append(args)
        elif pat.startswith(b"head"):
            cats["pick on head"] += 1
        else:
            cats["pick on a field"] += 1
    report(cats, others)


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = p.add_subparsers(dest="command", required=True)
    s = sub.add_parser("strict")
    s.add_argument("dump")
    s.add_argument("--a", required=True, help="mewl A, run again with joined values quoted")
    s.add_argument("--name-a", default="mewl")
    s.add_argument("--corpus-a", default="orig", choices=("orig", "lf", "lfdot", "clean"))
    s.add_argument("--work", default=os.path.join(HERE, "work"))
    s.set_defaults(fn=cmd_strict)
    s = sub.add_parser("bugfix")
    s.add_argument("dump")
    s.set_defaults(fn=cmd_bugfix)
    s = sub.add_parser("pick")
    s.add_argument("dump")
    s.set_defaults(fn=cmd_pick)
    opts = p.parse_args()
    opts.fn(opts)


if __name__ == "__main__":
    main()
