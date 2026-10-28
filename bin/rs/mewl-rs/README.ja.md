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
- pick (`-p head=val`): ヘッダ全体を、フィールドごとに復号する。ただし
  比較は (大文字小文字などを畳み込まず) そのまま行う。
- pick (`-p key=val` と `key!=val`) は、`--enable-utf8proc-char-fold` 付きで
  ビルドした `mewl` と同じく、大文字小文字以外の違いも無視する: 幅の
  ような互換性による違い ("ＡＢＣ" は "abc"、"ｱ" は "ア") と、異体字
  セレクタのようなデフォルトで無視できる符号位置である (NFKC_Casefold)。
  さらに、Windows が異なる対応付けをする JIS X 0208 の 7 文字 (下記参照)
  はどちらでも一致するので、メーラがどちらの表を使っていても、"〜" で
  "～" が、"～" で "〜" が見つかる。`key==val` と `key!==val` はこれらの
  どれも無視しない。
- encoded-word は、空白で区切られていなくても復号する。たとえば
  `Re:=?UTF-8?B?...?=`。
- 隣り合う同じ charset の encoded-word はまとめて変換するので、
  その間で分割されたマルチバイト文字も元に戻る。
- Mew が charset を推測するところでは、`mewl-rs` も推測する: ヘッダ中の
  UTF-8 でない生の文字列、たとえば昔の日本のメーラが符号化せずに書いた
  ISO-2022-JP や Shift_JIS と、`x-unknown` のような知らない charset、
  8 ビットのバイトを含む US-ASCII、ASCII の間に 8 ビットのバイトを含む
  UTF-8 の encoded-word である。Emacs は言語環境によって推測し、
  `mewl-rs` は Mew が渡すロケール (`LC_ALL`、`LC_CTYPE`、`LANG`) によって
  推測し、文字列を復号できた最初のものを採る: 日本語では UTF-8、
  ISO-2022-JP、EUC-JP、Shift_JIS (JIS X 0208 と 0212 のみ)、韓国語と
  中国語では UTF-8 と、言語に応じて EUC-KR、GB 2312、Big5 のいずれか、それ以外では
  UTF-8、C1 制御文字を除く ISO-8859-1、EUC-JP、Shift_JIS で、最後は
  ISO-8859-1 である。Emacs と違い、ISO-2022-JP を復号し、Mew が生の
  バイトとして表示する 8 ビットのバイトを含む US-ASCII も推測する。
- charset は Encoding Standard の名前か、そうでなければ glibc の名前、
  たとえば `cp949`、`cp936`、`latin9`、`ujis`、または Mew の名前
  (`unicode-1-1-utf-7`) で指定できる。Emacs が Unicode でその文字を持つ
  限り、Mew が、つまり Emacs が表示するとおりに変換し、それ以外は
  Encoding Standard の規定どおりに encoding_rs で変換する:
  - ISO-8859-1、ISO-8859-9、ISO-8859-11、TIS-620 は、Encoding Standard の
    ような windows-1252、windows-1254、windows-874 ではなく、それ自身で
    ある: ISO-8859-1 の 0x80 から 0x9F は C1 制御文字である。
  - Shift_JIS、EUC-JP、ISO-2022-JP では、JIS X 0208 の 7 文字は Windows の
    ものではなく JIS のものである: 0x2141 は "～" (U+FF5E) ではなく
    "〜" (U+301C) であり、0x2142、0x213D、0x215D、0x2171、0x2172、0x224C
    も同様である。Shift_JIS と CP932 (`windows-31j`)、EUC-JP と eucJP-ms は
    glibc と同じように区別し、CP932 と eucJP-ms は Emacs と同じく Windows
    のもののままである。
  - GB 2312 (GBK は除く) は、ISO-2022-JP-2 の中でも、0x2124 と 0x212A に
    "・" (U+30FB) と "―" (U+2015) を持つ。GB18030 は、Encoding Standard
    が私用領域とする 6 文字に CJK 統合漢字拡張 B の文字を持つ。EUC-KR は
    "㉾" (U+327E) を持つ。Big5 (Big5-HKSCS は除く) は 0xF9FE に "▓"
    (U+2593) を持つ。
  - Big5 の拡張のほとんどのように Emacs が私用領域の文字を持つところや、
    JIS X 0208 の NEC 特殊文字のように Unicode の文字を持たないところは、
    Encoding Standard のとおりである。たとえば "①"。
  `difftest/mewcheck.py` は、これを Emacs 上の Mew と符号ごとに比べる。
- encoded-word の charset や生の ISO-2022-JP で不正なバイト列は、
  Encoding Standard と同じく最大部分ごとに一つずつ U+FFFD (REPLACEMENT
  CHARACTER) に置き換える。`mewl` はそのバイトのまま残す。そのため、
  復号した値は常に UTF-8 であり、壊れた文字を含むフィールドも、バイト
  ごとではなく case folding と NFC で比べられる。
- UTF-7 と UTF-7-IMAP は、glibc とは違い、RFC 2152 と RFC 3501 の
  規定どおりに変換する。シフトされた並び (`+` または `&` から base64 の
  終わりまで) は、全体として正しい形のときだけ復号する。すなわち、
  サロゲートが対になっており、余るビットが 6 ビット未満でゼロであり、
  UTF-7-IMAP では `-` で終わり、印字可能な US-ASCII を符号化しておらず、
  別の並びの直後に続いていないこと。そうでなければそのまま残すので、
  表示されるのは復号した文字列か送られたままのものかのどちらかで、
  壊れた並びからできた文字になることはない。
  UTF-7 の `~` のように、それ自身を表してはならない US-ASCII の文字も
  同様に残す。US-ASCII にないバイトは U+FFFD になる。

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
- `-:` は未知のオプションであり、ヘルプは `-x` の引数を示し、パターンに
  `key==val` と `key!==val` を挙げる。

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

charset の名前は、glibc 付きの `mewl` と同じように解釈する。`mewl` は
`ks_c_5601-1987` を `CP949` にするように、いくつかの MIME の charset 名を
iconv 向けに読み替え、`shift_jis` を `CP932` で試すように、変換に失敗した
ときにいくつかを上位集合で再試行する。glibc は名前の大文字小文字と、
英字、数字、`_-.,:` 以外の文字を無視するので、`utf~8` は UTF-8 である。
また、`x-sjis` や `gb_2312` のような Encoding Standard の名前の一部を
知らない。glibc が知っている名前の一覧 `src/charset/glibc.rs` は
`tools/glibc-charsets.py` で作る。

1 バイトの charset は、同じスクリプトが glibc から取り出した glibc の
表で変換する。そのため ISO-8859-1 は Encoding Standard のような
windows-1252 ではなく C1 制御文字を持ち、US-ASCII には 0x7F を超える
バイトがなく、windows-1252 で未定義のバイトは不正である。Shift_JIS、
EUC-JP、CP932、eucJP-ms も、glibc と異なるところを修正した encoding_rs で、
glibc と同じように変換する: Shift_JIS と EUC-JP では、JIS X 0208 の 0x2141
("〜") は Windows のような U+FF5E ではなく U+301C (WAVE DASH) であり、
ほかの 5 文字も同様である。NEC 特殊文字と IBM 拡張文字は不正なので、
`mewl` は CP932 か eucJP-ms で再試行する。Shift_JIS の 0x5C と 0x7E は
"¥" と "‾" である。ISO-2022-JP、ISO-2022-JP-2、UTF-7、UTF-7-IMAP は、
不正な並びと glibc の表も含めて glibc と同じように変換し、このうち
ISO-2022-JP だけを Windows の表で定義する Encoding Standard には従わない。
たとえば "①" は ISO-2022-JP にはなく、glibc にない ISO-2022-JP-1 は
復号しない。それ以外の charset の変換は iconv ではなく
encoding_rs (Encoding Standard) による。Unicode の正規化は utf8proc では
なく unicode-normalization と caseless による。そのため、`mewl` と結果が
異なる場合が少しある:

- ほかのマルチバイトの charset、たとえば GBK、Big5、EUC-KR は Encoding
  Standard の表を使い、glibc の表とは異なる。
- glibc は知っているが encoding_rs と mewl-rs が知らない charset、たとえば
  UTF-32、UCS-4、ISO-2022-KR、ISO-2022-CN、JOHAB、EBCDIC の encoded-word は
  復号しない。
- UTF-16 と UCS-2 は UTF-16LE として扱い (UCS-2BE は UTF-16BE)、glibc が
  UTF-16 で読み取り、次の encoded-word まで持ち越すらしいバイト順マークは
  扱わない。また、不正な並びはそのまま残す。
- windows-1255 と windows-1258 は glibc の合成なしで変換する。`mewl` は
  iconv を flush しないので、後に続くものと合成されうる encoded-word の
  最後の文字を失う。"café" は "caf" になる。
- ISO-2022-JP-3 は ISO-2022-JP として変換し、glibc が知っている
  JIS X 0213 と JIS X 0201 のカタカナは扱わない。
- UTF-7 中の対にならない下位サロゲートは、それを補う文字を不正にする。
  glibc はこれを変換の次の段階に渡し、そこで iconv の内部に依存する形で
  拒否される。
- どちらかがまだ知らない新しい版の Unicode の文字は、正規化の結果が
  異なることがある。

ほかにも `mewl` と異なる点が 2 つあるが、Mew はどちらにも依存していない:

- `-v` は Mew のバージョンではなく、mewl-rs のバージョンを表示する。
- `mewl` は標準入力を複製してファイル記述子 3 とし、`-e` のコマンドは
  これを継承するが、mewl-rs はそうしない。
