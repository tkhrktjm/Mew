# mewl-rs

Mew のメッセージ・スキャナ `mewl` を Rust に移植したもの。

(注) この文書は、日本語の方が分かりやすい人のために
[README.md](README.md) を訳したものである。内容は英語の原本に従う。

## ビルド

```
% cargo build --release
```

バイナリは `target/release/mewl-rs` にできる。

## mewl-rs

`mewl-rs` として起動すると、入力、出力、オプションは `mewl` と同じで
ある (`mewl(1)` を参照)。

`mewl` と異なる点が 2 つあるが、Mew はどちらにも依存していない:

- `-v` は Mew のバージョンではなく、mewl-rs のバージョンを表示する。
- `mewl` は標準入力を複製してファイル記述子 3 とし、`-e` のコマンドは
  これを継承するが、mewl-rs はそうしない。

## mewl-rs-utf8

`mewl-rs-utf8` として起動すると、MIME で符号化されたヘッダの値
(RFC 2047) を復号し、UTF-8 に変換する。対象は次のとおり:

- sort (`-d field`): 表示する値。
- pick (`-p pattern`): 検索するヘッダ。たとえば日本語のパターンが、
  符号化された `Subject:` に一致するようになる。

scan の出力は Mew が復号するので、`mewl` と同じである。

この名前でリンクを作り:

```
% ln -s mewl-rs mewl-rs-utf8
```

Mew にこれを使うよう指定する:

```
(setq mew-prog-mewl "mewl-rs-utf8")
```
