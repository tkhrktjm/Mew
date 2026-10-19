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
