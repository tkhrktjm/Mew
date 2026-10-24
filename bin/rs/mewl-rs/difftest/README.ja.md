# difftest

mewl-rs を C 版の mewl と、あるいは mewl-rs どうしを比べる差分テスト。
二つのバイナリを、メッセージのコーパスに対して同じ引数と入力で実行し、
実行ごとに終了ステータス、標準出力、標準エラー出力を比べる。mewl-rs の
コミットログにある件数は、これで数えたものである。
mewl-rs には独自のバージョンがあるので、`-v` が表示するバージョンは
置き換えてから比べる。

(注) この文書は、日本語の方が分かりやすい人のために
[README.md](README.md) を訳したものである。内容は英語の原本に従う。

バイナリのほかに要るのは Python 3 だけである。

## ファイル

- `difftest.py`: テスト本体 (`difftest.py --help`)。
- `mkcorpus.py`: コーパスを作る。コーパスがまだないか、これより古い
  ときは、`difftest.py` がこれを実行して `work/` に作る。
- `classify.py`: `difftest.py --dump` が書き出した不一致を、それを
  説明するはずの変更ごとに分類する。
- `fake-inc`: `mewl -e` のための incm の代わり。

## バイナリのビルド

あるリビジョンの C 版 mewl を、iconv と、入っていれば utf8proc 付きで
ビルドする (Debian では libutf8proc-dev。main の mewl は、見つかれば
これを使う):

```
% mkdir /tmp/c && git archive 464013a | tar -x -C /tmp/c
% cd /tmp/c/bin && ./configure && make mewl
```

mewl-rs:

```
% cd bin/rs/mewl-rs && cargo build --release
```

## 実行

```
% cd bin/rs/mewl-rs/difftest
% ./difftest.py /tmp/c/bin/mewl ../target/release/mewl-rs --name-b mewl-rs-compat
```

mewl-rs の動作は起動したときの名前で決まるので、`--name-a` か `--name-b`
で名前を与える。`mewl-rs` は mewl より多く復号し、それ以外の名前、
たとえば `mewl` (既定) や `mewl-rs-compat` は mewl と同じである。

`--suite` でケースを選ぶ (既定は `wellformed` 以外のすべて):

| スイート | ケース数 | 内容 |
|---|---:|---|
| `base` | 3996 | オプション、フォルダと範囲、scan、sort (`-d`)、`-i`、`-e`、`-w`、エラー、ランダムに作った 3000 個の pick のパターン |
| `mime` | 1227 | RFC 2047 の encoded-word に対する pick と sort |
| `uni` | 1626 | Unicode の大文字小文字の畳み込みと NFC による pick |
| `utf7` | 471 | UTF-7 と UTF-7-IMAP の、正しい、および不正な encoded-word に対する pick と sort |
| `wellformed` | 3000 | 空のフォルダに対する、正しい形の pick のパターン |

`--corpus-a` と `--corpus-b` は、修正の影響を見分けるために、バイナリ
ごとにコーパスの変種を選ぶ:

| 変種 | 変更 |
|---|---|
| `orig` | なし |
| `lf` | 改行で終わらないメッセージすべての末尾に改行を足す |
| `lfdot` | `lf` に加えて、`.` で始まる最終行の後に一行足す |
| `clean` | 空のメッセージとヘッダのないメッセージを除き、すべてのヘッダを空行で終える |

`--dump FILE` は、`classify.py` のために不一致を `FILE` に追記する。

## 期待される結果

iconv と utf8proc 2.10 付きで (`--enable-utf8proc-char-fold` なしで)
ビルドした 464013a の C 版 mewl に対して、mewl-rs-compat は 7320 ケース
中 12 ケースで異なる。どれも `mime` スイートにあり、理由は一つである。
charset は iconv ではなく encoding_rs で変換するので、Shift_JIS の 0x8160
("〜") は U+301C ではなく U+FF5E になる。

```
% ./difftest.py /tmp/c/bin/mewl ../target/release/mewl-rs --name-b mewl-rs-compat
...
TOTAL: 7320 run, 12 mismatches
```

コーパスには UTF-7 の単独の下位サロゲートがない。mewl-rs はこれを不正と
するが、glibc は独自の扱いをする (src/utf7.rs を参照)。

## コミットログの件数の再現

下に挙げた各リビジョンの C 版 mewl と、シリーズの各コミットの mewl-rs を
ビルドする (ここで `rs/N` は、main の後の N 番目のコミットからビルドした
mewl-rs である)。コマンドはこのディレクトリで実行する。

コミットの欄はコミットログの件名で、結果の欄の "N run, M mismatches" は
`difftest.py` の出力のとおり「N ケース中 M ケースが不一致」を表す。

| コミット | コマンド | 結果 |
|---|---|---|
| 1. port mewl to Rust | `./difftest.py --suite base C-5cb062e rs/1` | 3996 run, 0 mismatches |
| 2. restructure as idiomatic Rust | `./difftest.py --suite base C-5cb062e rs/2` | 3996 run, 0 mismatches |
| 3. decode header values as mewl-rs-utf8 | `./difftest.py --suite base C-5cb062e rs/3` | 3996 run, 0 mismatches |
| 4. follow main in field names and decoding | `./difftest.py --suite base,mime C-fd5cdab rs/4` | 5223 run, 18 mismatches |
| 5. reshape into modules, act by the name | `./difftest.py --suite base,mime rs/4 rs/5 --name-b mewl-rs-compat` | 5223 run, 0 mismatches |
| | `./difftest.py --suite base,mime rs/4 rs/5 --name-a mewl-rs-utf8 --name-b mewl-rs` | 5223 run, 0 mismatches |
| | `./difftest.py --suite base,mime C-fd5cdab rs/5 --name-b mewl-rs-compat` | 5223 run, 18 mismatches |
| 6. keep the last line, parse patterns strictly | `./difftest.py --suite base,mime rs/5 rs/6 --name-a mewl-rs --name-b mewl-rs` | 5223 run, 1138 mismatches |
| | 同じものを `--corpus-a lfdot --corpus-b lfdot --dump 6.jsonl` 付きで実行し、続いて `./classify.py strict 6.jsonl --a rs/5 --name-a mewl-rs --corpus-a lfdot` | 272: 拒否 270、連結 2 |
| | 同じものを `--corpus-a lf --dump 6lf.jsonl` 付きで実行し、続いて `./classify.py strict 6lf.jsonl --a rs/5 --name-a mewl-rs --corpus-a lf` | 615: 最終行 343、拒否 270、連結 1、連結と最終行 1 |
| | `./difftest.py --suite wellformed rs/6 rs/6 --name-a mewl-rs-compat --name-b mewl-rs` | 3000 run, 0 mismatches |
| 7. fix the bugs that need no change of Mew | `./difftest.py --suite base,mime rs/6 rs/7 --name-a mewl-rs --name-b mewl-rs` | 5223 run, 1021 mismatches |
| | 同じものを `--corpus-a clean --corpus-b clean --dump 7.jsonl` 付きで実行し、続いて `./classify.py bugfix 7.jsonl` | 423: 不正な数 52、不正な範囲 43、":" のない `-d` 50、パターン中の 0xff 260、リスト中の NUL 16、`-h`/`-:` 2 |
| 8. follow main in Unicode matching | `./difftest.py --suite base,mime,uni C-868ed6f rs/8 --name-b mewl-rs-compat` | 6849 run, 24 mismatches |
| | `./difftest.py --suite base,mime,uni rs/7 rs/8 --name-a mewl-rs --name-b mewl-rs --dump 8.jsonl` を実行し、続いて `./classify.py pick 8.jsonl` | 253: すべてフィールドに対する pick |
| 10. convert UTF-7 as glibc does | `./difftest.py C-868ed6f rs/10 --name-b mewl-rs-compat` | 7320 run, 12 mismatches |
| | `./difftest.py --suite utf7 C-868ed6f rs/10 --name-b mewl-rs-compat` | 471 run, 0 mismatches |
| | `./difftest.py rs/9 rs/10 --name-a mewl-rs-compat --name-b mewl-rs-compat` | 7320 run, 196 mismatches: `+mime` で 12、`+utf7` で 184 |
| | 同じものを `--name-a mewl-rs --name-b mewl-rs` で | 7320 run, 240 mismatches: `+mime` で 20、`+utf7` で 220 |
| 11. convert UTF-7 as the RFCs specify in mewl-rs | `./difftest.py rs/10 rs/11 --name-a mewl-rs-compat --name-b mewl-rs-compat` | 7320 run, 0 mismatches |
| | 同じものを `--name-a mewl-rs --name-b mewl-rs` で | 7320 run, 122 mismatches、すべて `+utf7` |
| | `./difftest.py C-868ed6f rs/11 --name-b mewl-rs-compat` | 7320 run, 12 mismatches |
| 12. replace invalid sequences with U+FFFD in mewl-rs | `./difftest.py rs/11 rs/12 --name-a mewl-rs-compat --name-b mewl-rs-compat` | 7320 run, 0 mismatches |
| | 同じものを `--name-a mewl-rs --name-b mewl-rs` で | 7320 run, 20 mismatches: 不正なバイトを含むメッセージ +mime/26 と +utf7/20 |
| 14. follow main in reading no data left by the previous message | `./difftest.py C-464013a rs/14 --name-b mewl-rs-compat`、`--corpus-a lf --corpus-b lf` 付きでも | 7320 run, 12 mismatches |
| | `./difftest.py rs/13 rs/14 --name-a mewl-rs-compat --name-b mewl-rs-compat` | 7320 run, 182 mismatches、C-868ed6f と C-464013a の間と同数: +inbox/10 (空) と +inbox/23 (`Subj`) |
| | 同じものを `--name-a mewl-rs --name-b mewl-rs` で | 7320 run, 0 mismatches |

mewl-rs-compat が変わるのは、main に従う手順 4、8、14 と、UTF-7 を復号する
手順 10 だけである。それ以外の手順では前と同じで、たとえば、ともに
mewl-rs-compat として起動した rs/6 と rs/7 の間の不一致は 0 である。
