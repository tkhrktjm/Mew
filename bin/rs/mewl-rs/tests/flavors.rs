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
        let path = match prog {
            "mewl-rs" => PathBuf::from(env!("CARGO_BIN_EXE_mewl-rs")),
            "mewl-rs-compat" => self.dir.join("link/mewl-rs-compat"),
            _ => self.dir.join(prog),
        };
        let out = Command::new(path)
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
    /* mewl on glibc never matches a value beginning with a byte >= 0x80
    ignoring case. */
    if cfg!(all(target_os = "linux", target_env = "gnu")) {
        assert_eq!(f.run("mewl-rs-compat", &["-p", "subject=日本語", "+inbox"]), "");
    }
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
    /* head= of an empty message */
    assert_eq!(f.run("mewl-rs", &["-p", "head=stale", "+b"]), "1\n");
    assert_eq!(f.run("mewl-rs-compat", &["-p", "head=stale", "+b"]), "1\n2\n");
    /* a field name completed by what is left */
    assert!(f.run("mewl-rs", &["-f", "Subject:", "+b", "1-3"]).ends_with("Filename: 3\n\n.\n"));
    assert!(f.run("mewl-rs-compat", &["-f", "Subject:", "+b", "1-3"]).ends_with("Filename: 3\nSub\n\n.\n"));
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
    for (args, error) in [
        (&["-c", "-1", "+inbox"][..], "mewl-rs: invalid number '-1'.\n"),
        (&["-l", "x", "+inbox"], "mewl-rs: invalid number 'x'.\n"),
        (&["+inbox", "last:-3"], "mewl-rs: invalid range 'last:-3'.\n"),
        (&["-s", "+inbox 1-x"], "mewl-rs: invalid range '1-x'.\n"),
    ] {
        let args: Vec<_> = b.iter().chain(args).copied().collect();
        assert_eq!(f.run_raw("mewl-rs", &args), (false, String::new(), error.to_string()));
        assert!(f.run_raw("mewl-rs-compat", &args).0);
    }
    assert_eq!(f.run("mewl-rs", &["-d", "subject:", "+inbox", "-3"]), f.run("mewl-rs", &["-d", "subject:", "+inbox", "1-3"]));

    /* A number too large is not a message, while mewl counts it. */
    f.write("c/1", b"Subject: one\n\n");
    f.write("c/99999999999", b"Subject: large\n\n");
    assert!(f.run("mewl-rs", &["-f", "Subject:", "+c"]).starts_with("NumOfMsg: 1\n"));
    assert!(f.run("mewl-rs-compat", &["-f", "Subject:", "+c"]).starts_with("NumOfMsg: 2\n"));
}

#[test]
fn options() {
    let f = Fixture::new("options");
    assert_eq!(f.run_raw("mewl-rs", &["-:"]), (false, String::new(), "mewl-rs: unknown option '-:'.\n".to_string()));
    assert_eq!(f.run_raw("mewl-rs-compat", &["-:"]).2, "Usage: mewl-rs-compat [options] [folder [range]]\n");
    assert!(f.run_raw("mewl-rs", &["-h"]).2.contains("\n -x  <suffix>  Use this suffix.\n"));
    assert!(f.run_raw("mewl-rs-compat", &["-h"]).2.contains("\n -x            Use this suffix.\n"));
}
