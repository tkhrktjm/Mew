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

## テスト

`cargo test` は単体テストとバイナリのテストを実行する。`difftest/` は
メッセージのコーパスで mewl-rs と C 版の mewl を比べる。
difftest/README.ja.md を参照。

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

`mewl` のバグもいくつか直してある:

- 改行で終わらないメッセージの最終行を保つ。`mewl` は最後のバイトを
  改行で上書きするので、`last` が `las` になる。同様に、`.` で始まる本文の
  最終行も、ほかの行と同じくエスケープする。
- pick のパターンを厳密に解析する。値の後の引用されていない語は値の
  一部になるので、Mew が `subject='foo bar'` のつもりで渡す
  `subject=foo bar` は `foo bar` を見つける (`mewl` は `foo` を見つける)。
  `a=b c=d`、`(a=b c)`、`abc` のように一つの式になっていないパターンは
  エラー ("broken pattern.") になる (`mewl` は黙って最初の部分を評価する)。
  `=` が後に続く語は、値の一部ではなく、別の比較のキーである。
- メッセージを越えて読むことはない。`mewl` は前のメッセージがバッファに
  残したものを読むので、空のメッセージが `head=val` に一致したり、短い
  メッセージが持っていないフィールドを表示したりすることがある。
- 空行で始まるメッセージのヘッダは空である。`mewl` は本文をヘッダと
  みなす。
- `-i` のリスト中の NUL を含む行が、次の行を隠すことはない。
- 範囲や `-c`、`-l` の数は 10 進数でなければならない。`mewl` はゴミを 0 と
  読み、負の数を回り込ませるので、`last:-3` はすべてを意味する。エラーは
  "invalid range" または "invalid number" である。
- 番号が `unsigned int` に収まらないほど大きいファイルはメッセージでは
  ない。メッセージは通常のファイルでなければならない。
- `:` のない `-d key` の値が `:` で始まることはない。
- パターン中の 0xFF は 1 バイトであり、パターンの終わりではない。
- `-:` は未知のオプションであり、ヘルプは `-x` の引数を示す。

Mew の変更を要するバグは直していない。たとえば、Mew は `mewl` の終了
ステータスもエラーメッセージも見ないし、`&`、`|`、`(`、`)`、`!`、`"` を
含む値をそのまま渡す。

それ以外は `mewl-rs-compat` と同じである。scan の出力は Mew が復号するので、
`mewl` と同じである。Mew にこれを使うよう指定する:

```
(setq mew-prog-mewl "mewl-rs")
```

## mewl-rs-compat

`mewl-rs-compat`、または `mewl-rs` 以外の名前で起動すると、入力、出力、
オプションは `mewl` と同じである (`mewl(1)` を参照)。iconv と utf8proc
付きで (ただし `--enable-utf8proc-char-fold` なしで) ビルドした `mewl` と
同じく、pick (`-p key=val`) では:

- フィールド本体中の RFC 2047 の encoded-word を UTF-8 に復号する。
- `=` と `!=` は、フィールド本体と値を、Unicode の完全な case folding と
  正準合成 (NFC) の後で比べるので、"Straße" は "STRASSE" に一致する。
  `==` と `!==` は NFC の後だけで比べる。幅は畳み込まない。どちらかが
  正しい UTF-8 でなければ、ASCII 文字の大文字小文字だけを区別せずに
  バイトごとに比べる。キー `head` はそのまま比べる。

charset の変換は iconv ではなく encoding_rs (Encoding Standard) により、
Unicode の正規化は utf8proc ではなく unicode-normalization と caseless に
よるので、`mewl` と結果が異なる場合が少しある:

- Shift_JIS の 0x8160 ("〜") は U+FF5E になるが、glibc では U+301C である。
- Encoding Standard が知らない charset、たとえば UTF-7 の encoded-word は
  復号しない。
- どちらかがまだ知らない新しい版の Unicode の文字は、正規化の結果が
  異なることがある。

ほかにも `mewl` と異なる点が 2 つあるが、Mew はどちらにも依存していない:

- `-v` は Mew のバージョンではなく、mewl-rs のバージョンを表示する。
- `mewl` は標準入力を複製してファイル記述子 3 とし、`-e` のコマンドは
  これを継承するが、mewl-rs はそうしない。
