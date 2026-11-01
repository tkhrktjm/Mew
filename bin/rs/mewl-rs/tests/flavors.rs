//! `mewl-rs` decodes header values for sort and pick more than
//! `mewl-rs-compat`, which behaves as mewl. Which one it is depends on
//! the name it is invoked by, through a link for instance.

use std::fs;
use std::os::unix::fs::symlink;
use std::path::PathBuf;
use std::process::Command;

struct Fixture {
    dir: PathBuf,
}

impl Fixture {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("mewl-rs-test-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let inbox = dir.join("Mail/inbox");
        fs::create_dir_all(&inbox).unwrap();
        let messages: [&[u8]; 5] = [
            b"Subject: =?UTF-8?B?5pel5pys6Kqe44Gu5Lu25ZCN?=\nFrom: =?ISO-2022-JP?B?GyRCOzNFRBsoQg==?= <yamada@example.com>\n\nbody\n",
            b"Subject: =?ISO-2022-JP?B?GyRCRnxLXDhsGyhC?=\n =?ISO-2022-JP?B?GyRCJE5GfEtcGyhC?=\nFrom: foo@example.com\n\nbody\n",
            b"From-Address: x\nSubject: plain English\nFrom: bar@example.com\n\n\xe6\x97\xa5\xe6\x9c\xac\xe8\xaa\x9e in body\n",
            b"Subject: Re: =?Shift_JIS?B?k/qWe4zq?= (2)\n\nbody\n",
            b"Subject: Re:=?Shift_JIS?B?k/qW?= =?Shift_JIS?B?e4zq?=\n\nbody\n",
        ];
        for (i, m) in messages.iter().enumerate() {
            fs::write(inbox.join((i + 1).to_string()), m).unwrap();
        }

        let bin = env!("CARGO_BIN_EXE_mewl-rs");
        fs::create_dir(dir.join("link")).unwrap();
        for name in ["mewl-rs", "mewl-rs-compat", "mewl"] {
            symlink(bin, dir.join("link").join(name)).unwrap();
        }
        /* Like a hard link, which cannot be made across file systems. */
        fs::create_dir(dir.join("copy")).unwrap();
        fs::copy(bin, dir.join("copy/mewl-rs-compat")).unwrap();
        Fixture { dir }
    }

    /// Runs `mewl-rs`, or it through a link or a copy at `prog`, e.g.
    /// `link/mewl-rs-compat`. `mewl-rs-compat` is short for that.
    fn run(&self, prog: &str, args: &[&str]) -> String {
        self.run_in("C.UTF-8", prog, args)
    }

    /// [`Fixture::run`] in `locale`, by which `mewl-rs` guesses charsets.
    fn run_in(&self, locale: &str, prog: &str, args: &[&str]) -> String {
        let path = match prog {
            "mewl-rs" => PathBuf::from(env!("CARGO_BIN_EXE_mewl-rs")),
            "mewl-rs-compat" => self.dir.join("link/mewl-rs-compat"),
            _ => self.dir.join(prog),
        };
        let out = Command::new(path)
            .env("LC_ALL", locale)
            .arg("-b")
            .arg(self.dir.join("Mail"))
            .args(args)
            .output()
            .unwrap();
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        String::from_utf8(out.stdout).unwrap()
    }

    /// Runs `link/prog` with `args` as they are: (success, stdout, stderr).
    fn run_raw(&self, prog: &str, args: &[&str]) -> (bool, String, String) {
        let out = Command::new(self.dir.join("link").join(prog)).args(args).output().unwrap();
        let text = |b: Vec<u8>| String::from_utf8_lossy(&b).into_owned();
        (out.status.success(), text(out.stdout), text(out.stderr))
    }

    fn write(&self, path: &str, data: &[u8]) {
        let path = self.dir.join("Mail").join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, data).unwrap();
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

#[test]
fn sort_decodes_values() {
    let f = Fixture::new("sort");
    assert_eq!(
        f.run("mewl-rs", &["-l", "0", "-d", "subject:", "+inbox"]),
        "1: 日本語の件名\n2: 日本語の日本\n3: plain English\n4: Re: 日本語 (2)\n5: Re:日本語\n"
    );
    assert_eq!(
        f.run("mewl-rs-compat", &["-l", "0", "-d", "subject:", "+inbox", "1"]),
        "1: =?UTF-8?B?5pel5pys6Kqe44Gu5Lu25ZCN?=\n"
    );
    assert_eq!(f.run("mewl-rs", &["-d", "from:", "+inbox", "1"]), "1: 山田 <yamada@example.com>\n");
}

#[test]
fn pick_searches_decoded_values() {
    let f = Fixture::new("pick");
    assert_eq!(f.run("mewl-rs", &["-p", "subject=日本語", "+inbox"]), "1\n2\n4\n5\n");
    assert_eq!(f.run("mewl-rs", &["-p", "subject==日本語の日本", "+inbox"]), "2\n");
    assert_eq!(f.run("mewl-rs", &["-p", "from=山田 | subject=english", "+inbox"]), "1\n3\n");
    assert_eq!(f.run("mewl-rs", &["-p", "head=件名", "+inbox"]), "1\n");
    /* Only the header is searched. */
    assert_eq!(f.run("mewl-rs", &["-p", "head=\"日本語 in body\"", "+inbox"]), "");
}

#[test]
fn compat_decodes_fields_when_picking() {
    let f = Fixture::new("compat");
    /* An encoded-word has to be a word by itself, and is converted by
    itself. */
    assert_eq!(f.run("mewl-rs-compat", &["-p", "subject==日本語", "+inbox"]), "1\n2\n4\n");
    assert_eq!(f.run("mewl-rs-compat", &["-p", "from==山田", "+inbox"]), "1\n");
    /* but not the whole header. */
    assert_eq!(f.run("mewl-rs-compat", &["-p", "head==件名", "+inbox"]), "");
    assert_eq!(f.run("mewl-rs-compat", &["-p", "subject=日本語", "+inbox"]), "1\n2\n4\n");
}

#[test]
fn unicode_matching() {
    let f = Fixture::new("unicode");
    f.write("u/1", "Subject: Straße\n\n".as_bytes());
    f.write("u/2", "Subject: \u{c4}rger\n\n".as_bytes());
    f.write("u/3", "Subject: A\u{308}RGER\n\n".as_bytes());
    f.write("u/4", "Subject: ＡＢＣ\n\n".as_bytes());
    for prog in ["mewl-rs", "mewl-rs-compat"] {
        /* full case folding and NFC */
        assert_eq!(f.run(prog, &["-p", "subject=STRASSE", "+u"]), "1\n");
        assert_eq!(f.run(prog, &["-p", "subject=\u{e4}rger", "+u"]), "2\n3\n");
        /* NFC only */
        assert_eq!(f.run(prog, &["-p", "subject==A\u{308}rger", "+u"]), "2\n");
        /* the whole header is not normalized */
        assert_eq!(f.run(prog, &["-p", "head=STRASSE", "+u"]), "");
    }
    /* width is folded by mewl-rs only, and "==" folds nothing. */
    assert_eq!(f.run("mewl-rs-compat", &["-p", "subject=abc", "+u"]), "");
    assert_eq!(f.run("mewl-rs", &["-p", "subject=abc", "+u"]), "4\n");
    assert_eq!(f.run("mewl-rs", &["-p", "subject==ABC", "+u"]), "");
    /* nor the code points of Windows for JIS X 0208 */
    f.write("u/5", "Subject: =?UTF-8?B?44CcIO+9ng==?=\n\n".as_bytes());
    assert_eq!(f.run("mewl-rs", &["-p", "subject=〜", "+u"]), "5\n");
    assert_eq!(f.run("mewl-rs", &["-p", "subject=～", "+u"]), "5\n");
    assert_eq!(f.run("mewl-rs-compat", &["-p", "subject=～", "+u"]), "5\n");
    assert_eq!(f.run("mewl-rs", &["-p", "subject==〜 〜", "+u"]), "");
}

#[test]
fn key_names_a_field() {
    let f = Fixture::new("key");
    /* "subj" is not "Subject:", and "from" is not "From-Address:". */
    assert_eq!(f.run("mewl-rs-compat", &["-p", "subj=plain", "+inbox"]), "");
    assert_eq!(f.run("mewl-rs-compat", &["-p", "subject=plain", "+inbox"]), "3\n");
    assert_eq!(f.run("mewl-rs-compat", &["-p", "subject:=plain", "+inbox"]), "3\n");
    assert_eq!(f.run("mewl-rs-compat", &["-d", "from", "+inbox", "3"]), "3:: bar@example.com\n");
}

#[test]
fn invoked_by_name() {
    let f = Fixture::new("name");
    let args = ["-l", "0", "-d", "subject:", "+inbox"];
    let decoded = f.run("mewl-rs", &args);
    let compat = f.run("link/mewl-rs-compat", &args);
    assert_ne!(decoded, compat);
    /* mewl-rs through a link is still mewl-rs. */
    assert_eq!(f.run("link/mewl-rs", &args), decoded);
    /* Any other name, as a drop-in replacement, is mewl-rs-compat. */
    assert_eq!(f.run("link/mewl", &args), compat);
    assert_eq!(f.run("copy/mewl-rs-compat", &args), compat);
}

#[test]
fn scan_is_not_decoded() {
    let f = Fixture::new("scan");
    assert_eq!(f.run("mewl-rs", &["+inbox"]), f.run("mewl-rs-compat", &["+inbox"]));
    assert!(f.run("mewl-rs", &["+inbox", "1"]).contains("Subject: =?UTF-8?B?5pel5pys6Kqe44Gu5Lu25ZCN?=\n"));
}

#[test]
fn last_line_kept() {
    let f = Fixture::new("last");
    fs::write(f.dir.join("Mail/inbox/6"), b"Subject: no newline").unwrap();
    fs::write(f.dir.join("Mail/inbox/7"), b"Subject: x\n\nline\n.").unwrap();
    assert_eq!(f.run("mewl-rs", &["-d", "subject:", "+inbox", "6"]), "6: no newline\n");
    assert_eq!(f.run("mewl-rs-compat", &["-d", "subject:", "+inbox", "6"]), "6: no newlin\n\n");
    assert_eq!(f.run("mewl-rs", &["-p", "subject==newline", "+inbox"]), "6\n");
    assert_eq!(f.run("mewl-rs-compat", &["-p", "subject==newline", "+inbox"]), "");
    /* The last line of the body is escaped, too. */
    assert!(f.run("mewl-rs", &["+inbox", "7"]).ends_with("\nline\n..\n.\n"));
    assert!(f.run("mewl-rs-compat", &["+inbox", "7"]).ends_with("\nline\n\n.\n"));
}

#[test]
fn pattern_values_with_blanks() {
    let f = Fixture::new("blanks");
    /* Mew passes subject=plain English for subject='plain English'. */
    assert_eq!(f.run("mewl-rs", &["-p", "subject=plain English", "+inbox"]), "3\n");
    assert_eq!(f.run("mewl-rs", &["-p", "subject=plain Japanese", "+inbox"]), "");
    assert_eq!(f.run("mewl-rs-compat", &["-p", "subject=plain Japanese", "+inbox"]), "3\n");
}

#[test]
fn broken_patterns_refused() {
    let f = Fixture::new("broken");
    for (prog, pattern, expected) in [
        ("mewl-rs", "subject=plain from=zzz", Some("mewl-rs: broken pattern.\n")),
        ("mewl-rs", "(subject=plain from=zzz)", Some("mewl-rs: broken pattern.\n")),
        ("mewl-rs", "plain", Some("mewl-rs: broken pattern.\n")),
        ("mewl-rs-compat", "subject=plain from=zzz", None),
    ] {
        let out = Command::new(f.dir.join("link").join(prog))
            .args(["-b", f.dir.join("Mail").to_str().unwrap(), "-p", pattern, "+inbox"])
            .output()
            .unwrap();
        match expected {
            Some(message) => {
                assert_eq!(String::from_utf8_lossy(&out.stderr), message);
                assert!(!out.status.success());
            }
            None => assert_eq!((String::from_utf8_lossy(&out.stdout).as_ref(), out.status.success()), ("3\n", true)),
        }
    }
}

#[test]
fn no_data_left_by_the_previous_message() {
    let f = Fixture::new("stale");
    f.write("b/1", b"Subject: stale-data\n\n");
    f.write("b/2", b"");
    f.write("b/3", b"Subj");
    /* Neither reads it, as mewl does not since it ends what it reads
    with NUL: head= of an empty message, and a field name which what
    is left would complete. */
    for name in ["mewl-rs", "mewl-rs-compat"] {
        assert_eq!(f.run(name, &["-p", "head=stale", "+b"]), "1\n");
        assert!(f.run(name, &["-f", "Subject:", "+b", "1-3"]).ends_with("Filename: 3\n\n.\n"));
    }
}

#[test]
fn empty_header() {
    let f = Fixture::new("empty");
    f.write("b/1", b"\nSubject: in body\n");
    assert_eq!(f.run("mewl-rs", &["-p", "head=\"in body\"", "+b"]), "");
    assert_eq!(f.run("mewl-rs-compat", &["-p", "head=\"in body\"", "+b"]), "1\n");
    /* No field, and the body. mewl takes the body for the header. */
    assert!(f.run("mewl-rs", &["+b"]).contains("Filename: 1\n\nSubject: in body\n.\n"));
    assert!(f.run("mewl-rs-compat", &["+b"]).contains("Filename: 1\nSubject: in body\n\n.\n"));
}

#[test]
fn nul_in_a_list() {
    let f = Fixture::new("nul");
    fs::write(f.dir.join("list"), b"1\0junk\n3\n1\n").unwrap();
    let list = f.dir.join("list");
    let args = ["-d", "subject:", "-i", list.to_str().unwrap(), "+inbox"];
    assert_eq!(f.run("mewl-rs", &args), "1: 日本語の件名\n3: plain English\n1: 日本語の件名\n");
    assert_eq!(f.run("mewl-rs-compat", &args), "1: =?UTF-8?B?5pel5pys6Kqe44Gu5Lu25ZCN?=\n1: =?UTF-8?B?5pel5pys6Kqe44Gu5Lu25ZCN?=\n");
}

#[test]
fn sort_key_without_colon() {
    let f = Fixture::new("colon");
    assert_eq!(f.run("mewl-rs", &["-d", "from", "+inbox", "3"]), "3: bar@example.com\n");
    assert_eq!(f.run("mewl-rs-compat", &["-d", "from", "+inbox", "3"]), "3:: bar@example.com\n");
}

#[test]
fn numbers() {
    let f = Fixture::new("numbers");
    let mail = f.dir.join("Mail");
    let b = ["-b", mail.to_str().unwrap()];
    for (args, error, compat) in [
        (&["-c", "-1", "+inbox"][..], "mewl-rs: invalid number '-1'.\n", Some("negative number: -1")),
        (&["-l", "x", "+inbox"], "mewl-rs: invalid number 'x'.\n", None),
        (&["+inbox", "last:-3"], "mewl-rs: invalid range 'last:-3'.\n", Some("negative number: -3")),
        (&["-s", "+inbox 1-x"], "mewl-rs: invalid range '1-x'.\n", None),
        (&["-l", "4294967296", "+inbox"], "mewl-rs: invalid number '4294967296'.\n", Some("number out of range: 4294967296")),
    ] {
        let args: Vec<_> = b.iter().chain(args).copied().collect();
        assert_eq!(f.run_raw("mewl-rs", &args), (false, String::new(), error.to_string()));
        let (ok, _, stderr) = f.run_raw("mewl-rs-compat", &args);
        match compat {
            Some(e) => assert_eq!((ok, stderr), (false, format!("mewl-rs-compat: {e}\n"))),
            None => assert!(ok),
        }
    }
    assert_eq!(f.run("mewl-rs", &["-d", "subject:", "+inbox", "-3"]), f.run("mewl-rs", &["-d", "subject:", "+inbox", "1-3"]));
    assert!(f.run_raw("mewl-rs", &b.iter().chain(&["-l", "4294967295", "+inbox"]).copied().collect::<Vec<_>>()).0);

    /* A number too large for unsigned int is not a message, and one
    over INT_MAX is. */
    f.write("c/1", b"Subject: one\n\n");
    f.write("c/3000000000", b"Subject: large\n\n");
    f.write("c/99999999999", b"Subject: too large\n\n");
    for name in ["mewl-rs", "mewl-rs-compat"] {
        assert_eq!(f.run(name, &["-d", "subject:", "+c"]), "1: one\n3000000000: large\n", "{name}");
    }
}

#[test]
fn options() {
    let f = Fixture::new("options");
    assert_eq!(f.run_raw("mewl-rs", &["-:"]), (false, String::new(), "mewl-rs: unknown option '-:'.\n".to_string()));
    assert_eq!(f.run_raw("mewl-rs-compat", &["-:"]).2, "Usage: mewl-rs-compat [options] [folder [range]]\n");
    assert!(f.run_raw("mewl-rs", &["-h"]).2.contains("\n -x  <suffix>  Use this suffix.\n"));
    assert!(f.run_raw("mewl-rs-compat", &["-h"]).2.contains("\n -x            Use this suffix.\n"));
    assert!(f.run_raw("mewl-rs", &["-h"]).2.contains("\n<pattern> = 'key=val' | 'key!=val' | 'key==val' | 'key!==val'\n          | '!<pattern>' | '(<pattern>)' \n"));
    assert!(f.run_raw("mewl-rs-compat", &["-h"]).2.contains("\n<pattern> = 'key=val' | 'key!=val' | '!<pattern>' | '(<pattern>)' \n          | '<pattern>&<pattern>' \n"));
}

#[test]
fn guessing_charsets() {
    let f = Fixture::new("guess");
    f.write("g/1", b"Subject: \x93\xfa\x96\x7b\x8c\xea\nFrom: \xc6\xfc\xcb\xdc <x@example.com>\n\n");
    f.write("g/2", b"Subject: =?x-unknown?B?k/qWe4zq?=\n\n");
    f.write("g/3", b"Subject: =?us-ascii?Q?=93=FA=96=7B?=\n\n");
    f.write("g/4", b"Subject: =?UTF-8?Q?caf=E9?=\n\n");
    /* raw Shift_JIS and EUC-JP, Shift_JIS named unknown or US-ASCII, in Japanese */
    assert_eq!(f.run_in("ja_JP.UTF-8", "mewl-rs", &["-p", "subject=日本", "+g"]), "1\n2\n3\n");
    assert_eq!(f.run_in("ja_JP.UTF-8", "mewl-rs", &["-p", "from=日本", "+g"]), "1\n");
    assert_eq!(f.run_in("ja_JP.UTF-8", "mewl-rs", &["-d", "subject:", "+g", "2"]), "2: 日本語\n");
    /* in another language, ISO-8859-1 if it has no C1 control, which
    Shift_JIS has and EUC-JP has not, as Emacs */
    assert_eq!(f.run_in("C.UTF-8", "mewl-rs", &["-p", "subject=日本", "+g"]), "1\n2\n3\n");
    assert_eq!(f.run_in("C.UTF-8", "mewl-rs", &["-p", "from=Æü", "+g"]), "1\n");
    assert_eq!(f.run_in("C.UTF-8", "mewl-rs", &["-p", "subject=café", "+g"]), "4\n");
    /* mewl-rs-compat guesses nothing. */
    assert_eq!(f.run_in("ja_JP.UTF-8", "mewl-rs-compat", &["-p", "subject=日本", "+g"]), "");
}
