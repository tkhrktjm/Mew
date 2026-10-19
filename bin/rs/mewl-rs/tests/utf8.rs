//! `mewl-rs-utf8` decodes header values for sort and pick, while
//! `mewl-rs` behaves as mewl.

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
        let messages: [&[u8]; 4] = [
            b"Subject: =?UTF-8?B?5pel5pys6Kqe44Gu5Lu25ZCN?=\nFrom: =?ISO-2022-JP?B?GyRCOzNFRBsoQg==?= <yamada@example.com>\n\nbody\n",
            b"Subject: =?ISO-2022-JP?B?GyRCRnxLXDhsGyhC?=\n =?ISO-2022-JP?B?GyRCJE5GfEtcGyhC?=\nFrom: foo@example.com\n\nbody\n",
            b"Subject: plain English\nFrom: bar@example.com\n\n\xe6\x97\xa5\xe6\x9c\xac\xe8\xaa\x9e in body\n",
            b"Subject: Re: =?Shift_JIS?B?k/qWe4zq?= (2)\n\nbody\n",
        ];
        for (i, m) in messages.iter().enumerate() {
            fs::write(inbox.join((i + 1).to_string()), m).unwrap();
        }
        for name in ["mewl-rs", "mewl-rs-utf8"] {
            symlink(env!("CARGO_BIN_EXE_mewl-rs"), dir.join(name)).unwrap();
        }
        Fixture { dir }
    }

    fn run(&self, prog: &str, args: &[&str]) -> String {
        let out = Command::new(self.dir.join(prog))
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
        f.run("mewl-rs-utf8", &["-l", "0", "-d", "subject:", "+inbox"]),
        "1: 日本語の件名\n2: 日本語の日本\n3: plain English\n4: Re: 日本語 (2)\n"
    );
    assert_eq!(
        f.run("mewl-rs", &["-l", "0", "-d", "subject:", "+inbox", "1"]),
        "1: =?UTF-8?B?5pel5pys6Kqe44Gu5Lu25ZCN?=\n"
    );
    assert_eq!(f.run("mewl-rs-utf8", &["-d", "from:", "+inbox", "1"]), "1: 山田 <yamada@example.com>\n");
}

#[test]
fn pick_searches_decoded_values() {
    let f = Fixture::new("pick");
    assert_eq!(f.run("mewl-rs-utf8", &["-p", "subject=日本語", "+inbox"]), "1\n2\n4\n");
    assert_eq!(f.run("mewl-rs-utf8", &["-p", "subject==日本語の日本", "+inbox"]), "2\n");
    assert_eq!(f.run("mewl-rs-utf8", &["-p", "from=山田 | subject=english", "+inbox"]), "1\n3\n");
    assert_eq!(f.run("mewl-rs-utf8", &["-p", "head=件名", "+inbox"]), "1\n");
    /* Only the header is searched. */
    assert_eq!(f.run("mewl-rs-utf8", &["-p", "head=\"日本語 in body\"", "+inbox"]), "");
    /* mewl finds nothing. */
    assert_eq!(f.run("mewl-rs", &["-p", "subject=日本語", "+inbox"]), "");
}

#[test]
fn scan_is_not_decoded() {
    let f = Fixture::new("scan");
    assert_eq!(f.run("mewl-rs-utf8", &["+inbox"]), f.run("mewl-rs", &["+inbox"]));
    assert!(f.run("mewl-rs-utf8", &["+inbox", "1"]).contains("Subject: =?UTF-8?B?5pel5pys6Kqe44Gu5Lu25ZCN?=\n"));
}
