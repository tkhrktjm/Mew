# mewl-rs

Mew のメッセージ・スキャナ `mewl` を Rust に移植したもの。

(注) この文書は、日本語の方が分かりやすい人のために
[README.md](README.md) を訳したものである。内容は英語の原本に従う。

## ビルド

```
% cargo build --release
```

バイナリは `target/release/mewl-rs` にできる。`cargo install --path .`
でインストールできる。

動作は起動したときの名前によって変わる:

- `mewl-rs`: MIME で符号化されたヘッダの値を `mewl` よりも多く復号する。
- それ以外の名前、たとえば `mewl-rs-compat`: `mewl` と同じ。

そこで、もう一方の名前でリンク (シンボリック・リンクでもハード・リンク
でもよい) を作る:

```
% ln -s mewl-rs mewl-rs-compat
```

`mewl` という名前のリンクにすれば、`mewl` をそのまま置き換えられる。

## mewl-rs

`mewl-rs` として起動すると、`mewl` よりも多くを復号する:

- sort (`-d field`): 表示する値を復号する。
- pick (`-p head=val`): ヘッダ全体を、フィールドごとに復号する。
- encoded-word は、空白で区切られていなくても復号する。たとえば
  `Re:=?UTF-8?B?...?=`。
- 隣り合う同じ charset の encoded-word はまとめて変換するので、
  その間で分割されたマルチバイト文字も元に戻る。
- 昔の日本のメーラが符号化せずに書いた、ヘッダ中の生の ISO-2022-JP を
  UTF-8 に変換する。
- UTF-8 の値は大文字小文字を区別せずに一致する (`key=val`)。glibc 上の
  `mewl` は、0x80 以上のバイトで始まる値に大文字小文字を区別せずに一致する
  ことがないので、`subject=日本語` は何も見つけない。

それ以外は `mewl-rs-compat` と同じである。scan の出力は Mew が復号するので、
`mewl` と同じである。Mew にこれを使うよう指定する:

```
(setq mew-prog-mewl "mewl-rs")
```

## mewl-rs-compat

`mewl-rs-compat`、または `mewl-rs` 以外の名前で起動すると、入力、出力、
オプションは `mewl` と同じである (`mewl(1)` を参照)。iconv 付きで
ビルドした `mewl` と同じく、pick (`-p key=val`) では、フィールド本体中の
RFC 2047 の encoded-word を UTF-8 に復号する。

charset の変換は iconv ではなく encoding_rs (Encoding Standard) によるので、
`mewl` と結果が異なる場合が少しある:

- Shift_JIS の 0x8160 ("〜") は U+FF5E になるが、glibc では U+301C である。
- Encoding Standard が知らない charset、たとえば UTF-7 の encoded-word は
  復号しない。

ほかにも `mewl` と異なる点が 2 つあるが、Mew はどちらにも依存していない:

- `-v` は Mew のバージョンではなく、mewl-rs のバージョンを表示する。
- `mewl` は標準入力を複製してファイル記述子 3 とし、`-e` のコマンドは
  これを継承するが、mewl-rs はそうしない。
