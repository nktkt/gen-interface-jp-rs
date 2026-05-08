# Gen Interface JP — アーキテクチャ

## 概要

Gen Interface JP はフォントビルドパイプライン (アプリ/UI なし)。`just`
ターゲット経由で vendor のソースから配布用 TTF / WOFF2 / npm 成果物を
生成する。各ウェイトは Rust の 3 ステージを通り、最後に静的デモサイトが
公開済みの webfont パッケージを参照する。

```
┌────────────────────────────────────────────────────────────────────┐
│  Source                                                            │
│    vendor/fonts/Inter-4.1/extras/ttf/Inter-{Weight}.ttf            │
│    vendor/fonts/Inter-4.1/extras/ttf/InterDisplay-{Weight}.ttf     │
│    vendor/fonts/Noto_Sans_JP/NotoSansJP-VariableFont_wght.ttf      │
└─────────────────────────────┬──────────────────────────────────────┘
                              │
        ┌─────────────────────▼──────────────────────────┐
        │  gen-font::build  (ファミリー × ウェイトごと)    │
        │                                                 │
        │   [1/3] Bake — gen_font::baker, base-only       │
        │         Noto wght → static TTF                  │
        │         metadataMode=inheritBase                │
        │             ↓                                   │
        │   [2/3] Proportionalise — proportional.rs       │
        │         palt → hmtx (3 バケット方針)              │
        │         tracking + 極端な bbox の除去              │
        │             ↓                                   │
        │   [3/3] Merge — gen_font::baker                 │
        │         Inter (sub) + proportional Noto (base)  │
        │         subFont.excludeCodepoints で日本語慣習    │
        │         記号は Noto を維持                        │
        │         metricsSource=sub, manufacturer 刻印     │
        │             ↓                                   │
        │   dist/ttf/  (ファミリー × ウェイトごとに TTF)    │
        │                                                  │
        └─────────────────────┬───────────────────────────┘
                              │
        ┌─────────────────────▼──────────────────────────┐
        │  gen-webfont::build — unicode-range サブセット化  │
        │     google-japanese ストラテジー (デフォルト) →    │
        │     all.css + ウェイト別 CSS + WOFF2 チャンク      │
        │     dist/webfont/gen-interface-jp/              │
        └─────────────────────┬───────────────────────────┘
                              │
        ┌─────────────────────▼──────────────────────────┐
        │  gen-release::build — パッケージング             │
        │     dist/release/github/   → GitHub Releases    │
        │     dist/release/npm/      → npm publish        │
        │     dist/release/webfonts/ → GitHub Pages       │
        └─────────────────────┬───────────────────────────┘
                              │
        ┌─────────────────────▼──────────────────────────┐
        │  site/  — Vite 静的デモサイト                     │
        │     jsDelivr (npm CDN) 経由で webfont を読込      │
        │     GitHub Pages へデプロイ                      │
        └─────────────────────────────────────────────────┘
```

## データフロー

### 1. フォントウェイトのビルド (`gen-font::build`)

```
FAMILIES × WEIGHTS の各組合せに対して:
  → gen_font::baker bake: variable Noto → static TTF
                    inheritBase で designer/OFL/version を継承
                    weight だけを上書き
  → inst を再読込し、キャッシュした variable から palt 取得
  → グリフを kana letters / reduced palt / squeeze SB に分類
    (CJK 漢字と vert-only グリフは除外)
  → make_proportional で palt → hmtx に焼き込み
    palt/vpal/halt/vhal 削除
  → apply_tracking で advance を広げ LSB を半分シフト
  → apply_glyph_spacing で family.glyph_spacing の個別調整を適用
  → strip_extreme_glyphs で繰り返し記号 〱〲 を無効化
    (yMax > 1200 / yMin < -400)
  → gen_font::baker merge: Inter + proportional Noto
                     subFont.excludeCodepoints = SUB_EXCLUDE_CODEPOINTS で
                     日本語慣習記号 (① Ⓐ ※ ◯ …) は Noto を維持
                     glyph-name collision (Inter U+0298 と Noto U+25CE が
                     共に `uni25CE`) も baker が自動 rename
                     family/weight を「Gen Interface JP …」に刻印
                     metricsSource=sub で Inter 基準の hhea を採用
                     manufacturer / manufacturerURL を刻印
```

### 2. Web 用サブセット化 (`gen-webfont::build`)

```
dist/ttf/{family}/{family}-{weight}.ttf を読み込み
  → プランナーがストラテジー選択:
      google-japanese (デフォルト) — Google Fonts の日本語
                                   unicode-range スライスを再現
                                   (~120 チャンク)
      gen              — 手動設計のスライス計画
  → 各 (family × weight × slice) に対して:
      skrifa + write-fonts でサブセット → woff2 crate で圧縮
      .nam ファイル → 人間可読のコードポイント一覧
  → スライスごとに `unicode-range:` 付き @font-face を生成
  → all.css (フルファミリー) + ウェイト別 CSS を出力
  → manifest.json にサイズ / brotli サイズを記録
```

### 3. パッケージング & 公開 (`gen-release::build`)

```
dist/ttf/ + dist/webfont/gen-interface-jp/ を要求
  → GenInterfaceJP-<version>.zip (TTF、全ウェイト × 両ファミリー)
  → webfont package を npm/      にコピー (package.json 同梱)
  → webfont package を webfonts/ にコピー (Pages 配信用ミラー)
  → manifest.json に version, tag, アセット URL を記録
```

## ビルドパイプライン (`crates/gen-font/src/build.rs`)

### Stage 1 — variable から static へベイク

`gen_font::baker` を Noto Variable に対して base-only で実行。wght 軸は
ファミリーごとのウェイト値に固定 (`Regular = 465`、`SemiBold = 690` のように
非整数: Noto の軸は非線形で、整数位置の太さでは Inter より細く見えるため
目視で合わせ込んだ値)。`output.metadata_mode = MetadataMode::InheritBase` で
Noto の identity レコード (designer / OFL / manufacturer / version) を
そのまま通すので、inst TTF は手動 save/restore なしにクリーンな出元
メタデータを持って Stage 2 に渡る。

### Stage 2 — プロポーショナル化 + メトリクス調整

inst に対して 4 つのサブパスを in-place で実行:

1. **palt のベイク** (`proportional::make_proportional`) — palt 値は
   キャッシュ済みの variable から読む (instantiation で非デフォルト軸位置の
   palt ValueRecord が壊れることがあるため)。XPlacement / XAdvance を
   LSB / advance に加算しアウトラインをシフト。3 バケット:
   - **kana letters** — palt 全量
   - **reduced palt** (句読点等、デフォルト ⅓) — palt 対象だが kana letter
     ではないグリフ
   - **squeeze SB** — palt 非対象、非 kana、非 CJK、非 vertical なグリフ;
     LSB と RSB をそれぞれ `1 - squeeze_sb_scale` 分縮める
   CJK 漢字と `vert` / `vrt2` の代替グリフは除外 — 全角メトリクスを保つ。
2. **トラッキング** (`apply_tracking`) — advance を `tracking` 分広げ、
   `tracking / 2` を LSB に加算してアウトラインを広がった枠の中央に
   配置。kana / 句読点はファミリー設定の `tracking_kana` で別値。
3. **個別グリフのスペーシング** (`apply_glyph_spacing`) — palt + 一律
   トラッキングだけでは追い込めない稀なグリフのための手動レイヤー。
   ファミリー設定の `glyph_spacing` がコードポイント (または 1 文字) を
   `(lsb_delta, rsb_delta)` ペアにマップする: `lsb_delta` はアウトラインを
   スロット内で右にシフトしつつ advance を同量広げ、`rsb_delta` は
   advance を右側だけ広げる。アウトライン座標は触らない。各エントリは
   特定グリフを特定の隣接リズムに対して個別チューニングする想定なので、
   慎重に追加すること。現在の調整値は `crates/gen-font/src/build.rs` の
   `FAMILIES` を参照。
4. **bbox 除去** (`strip_extreme_glyphs`) — 下記 [垂直メトリクス] 参照。

オプションの **横スケール** (`x_scale` 設定、現在未使用) は上記の後に
動き、CJK を縦方向は触らず横だけ縮める。

### Stage 3 — Inter とマージ

`gen_font::baker` のマージモード: Inter が sub、プロポーショナル Noto が
base。`sub_font.exclude_codepoints = SUB_EXCLUDE_CODEPOINTS` で日本語慣習
として Noto 由来で残したい記号 (`①` `Ⓐ` `※` `◯` …) を列挙すると、baker は
マージ前に Inter の cmap から該当エントリを剥がし、base のグリフを生かす。
baker はさらに **クロスコードポイントなグリフ名衝突** も自動検出する:
Inter の U+0298 (`ʘ`) と Noto の U+25CE (`◎`) は両方 `uni25CE` という
glyph 名で出荷されているため、放置すると Inter 側が `◎` を上書きしてしまう。
baker は sub のほうを `uni25CE.sub` にリネームし、base のグリフを温存する。
この 2 段で、直接重複と命名衝突の両方を、こちら側で cmap を手術せずに
カバーできる。

`output.metrics_source = MetricsSource::Sub` で merged の hhea / OS/2 包絡線
を Inter 側に揃え、欧文のメトリクスが行高を駆動する。`BASELINE_OFFSET = 25`
で Noto を上に持ち上げ、CJK 漢字が Latin の caps と光学的に同じベースライン
に乗るように調整。`SCALE = 0.925` で Noto を縮め、CJK 1 文字の幅が Inter の
cap-height と揃う — 欧文/CJK 混植で CJK を少し小さくして釣り合いを取る、
という慣例的な配分。

`output.manufacturer = "Yamato Iizuka"`、`output.manufacturer_url =
"https://yamatoiizuka.com"` でリリース TTF の nameID 8 / 11 を刻印。

## プロポーショナルメトリクス (`crates/gen-font/src/proportional.rs`)

CJK フォントは全角がデフォルト: 全グリフがアウトライン幅に関係なく同じ
em-square を占有し、`palt` GPOS が runtime に kana / Latin を光学的に
詰める。`palt` を有効にしないアプリ (Adobe の和文コンポーザー、ブラウザ
フォールバック、CJK を等幅扱いするレイアウトエンジン) ではこの調整が
効かず、全角ピッチで組まれてしまう。

`make_proportional` は `palt` を static の `hmtx` に焼き込み、その上で
`palt` / `vpal` / `halt` / `vhal` を削除する — `palt` を効かせるアプリで
二重適用されるのを防ぐため。TrueType アウトラインのみ対応 (palt のベイクは
`glyf` に書き戻すので CFF は対象外)。

`remove_prop_features` は GPOS を 2 段で歩く: FeatureRecord の削除と、
それに対応する LangSys インデックスの再マップ。レコード削除は後ろの全
レコードのインデックスを動かすので、各 LangSys の `feature_indices`
配列を生き残ったレコードに対して再キーする必要がある。Lookup テーブル
本体は触らない — palt の lookup は他の残す機能からも参照される可能性
があり、孤立した lookup は無害なのでそのまま。

## 垂直メトリクスと Illustrator のテキストボックス問題

### 背景

Illustrator では、CJK グリフを含むフォントは強制的に **Japanese コンポーザー**
で扱われ、行送りが「ポイントサイズ × 固定倍率(170%前後)」になる。
Inter の Latin 専用挙動 (各行のグリフに応じた行高動的調整) はフォント側
から制御不能 (Illustrator の仕様)。

ただし、**テキストフレームの自動サイズ**は `head.yMax` / `head.yMin` を
参照しているため、ここを縮められれば少なくともテキストフレームの上下
余白は小さくなる。

### 削除するグリフ

`strip_extreme_glyphs` は `yMax > 1200` または `yMin < -400` (em = 1000
基準) のグリフを無効化する。Noto Sans JP では実質、縦組み用イテレーション
マークと `vert` / `vrt2` 代替に限定される。

| Glyph | Codepoint | 削除理由 |
|---|---|---|
| `uni3031` 〱 | U+3031 | 縦組み用繰り返し記号 |
| `uni3032` 〲 | U+3032 | 縦組み用濁点付き繰り返し記号 |
| (vert alternate) | (unmapped) | `uni3031` の vert/vrt2 代替 |
| (vert alternate) | (unmapped) | `uni3032` の vert/vrt2 代替 |

スロット自体は残してアウトラインを空にするので GSUB / GPOS のインデックス
は崩れない。cmap エントリは落とすので、コードポイントを直接打つと
.notdef にフォールスルーする。

| | yMin / yMax | span |
|---|---|---|
| Before (Noto そのまま) | -1047 / +1807 | 2.85×em |
| After | 約 -319 / +1108 | ~1.43×em (Inter 相当) |

### 設計方針 — UI フォントとして横組み専用

本フォントは **UI・本文用途の横書き専用** として設計する。

- **縦組み・伝統的な日本語組版は非対応。**
- em-square 厳密準拠 (Hiragino 式 hhea = 880 / -120) は追求しない —
  `metrics_source: Sub` で Inter の比率 (~1.21×em) を継承しているため、
  ベトナム語・ダイアクリティカル付き Latin (~1.11×em) が切り詰めで欠ける。
- トレードオフは受け入れる: head bbox を削って Illustrator のフレーム
  自動サイズを改善する。縦組みイテレーションマーク 〱〲 はこのフォントで
  描画されないが、UI 用途では使わない、という判断。

## Webfont サブセット化 (`crates/gen-webfont/src/build.rs`)

`gen-font::build` の TTF をそのまま Web に乗せるには大きすぎる (1 ウェイト
あたり約 5 MB)。`gen-webfont::build` は各ウェイトを Unicode の範囲で
スライスし、各スライスに対して `unicode-range:` 付き `@font-face` を 1 つ
出す。ブラウザはページのテキストが参照したチャンクだけをダウンロードする。

### ストラテジー

- **`google-japanese`** *(デフォルト)* — Google Fonts の日本語スライス
  方式 (`vendor/nam-files/slices/japanese_default.txt`) を再現。Google
  ホスティングの Noto と同じチャンク境界を使うので、カバレッジとキャッシュ
  挙動が既存の日本語サイトと整合する。
- **`gen`** — 手動設計のプラン: Latin / kana / 句読点 / JIS 16-92 区 /
  残余漢字を `extra_han_slices` で均等分割。

### 出力

```
dist/webfont/gen-interface-jp/
  all.css                # 全ウェイト × 両ファミリー
  400.css                # normal Regular (ウェイトごとに 1 ファイル)
  display-400.css        # display Regular (ウェイトごとに 1 ファイル)
  ...
  w/{family}/{weight}/{slice}.woff2
  nam/{slice}.nam        # 人間可読のコードポイント一覧
  manifest.json          # スライスごとのサイズ / brotli サイズ
```

`benchmark.mjs` (Node) はローカルサブセットに対する throttled fetch を
再生し、スライス分割が単一フル WOFF2 比でペイするかを検証する。
比較対象のフル WOFF2 は Regular TTF からオンデマンドで生成する
(`gen-webfont::build` の `--all` なしモード経由)。リリース成果物には
含めない。

## リリースパッケージング (`crates/gen-release/src/build.rs`)

下流のコンシューマーが 3 種類、出力も 3 種類:

- **GitHub Releases** (`dist/release/github/`) —
  `GenInterfaceJP-<version>.zip` 1 本に TTF 全 16 本 (両ファミリー × 8
  ウェイト) を同梱。アセット名にバージョンが埋め込まれているので、より
  新しいリリースが「latest」になった後でも各リリースを一意にリンクできる。
  フル WOFF2 単一ファイルは意図的に再配布しない — Web 配信は下記 npm
  サブセット経由が本道、自前ホスティングする場合も TTF→WOFF2 変換は
  `skrifa` + `write-fonts` + `woff2` crate で容易。
- **npm パッケージ** (`dist/release/npm/`) — webfont サブセット +
  自動生成された `package.json` (name, version, files, OFL-1.1 license)。
  jsDelivr がパッケージのルートから `all.css` とウェイト別 CSS を配信する。
- **GitHub Pages ミラー** (`dist/release/webfonts/gen-interface-jp/`) —
  npm と同じ webfont パッケージを、デモサイトの隣に静的ファイルとして配信。

バージョンは `Cargo.toml` (CI では `GITHUB_REF_NAME`) から読む。
github / npm / webfonts ディレクトリの隣にある `manifest.json` には
リリース URL が記録され、下流ツールが参照できる。

## サイト (`site/`)

`site/` 配下の Vite 静的サイト。実行時に jsDelivr の npm CDN 経由で公開
済み webfont パッケージをロードする — つまりライブサイトはサードパーティ
コンシューマーが使うのと同じ npm 成果物を使い、エンドツーエンドでパッケージ
の動作確認になる。GitHub Pages のデプロイは `.github/workflows/pages.yml`
で実行。

## テスト

```bash
cargo test                              # 全テスト (ワークスペース全体)
cargo test -p gen-font                  # 単一クレート
```

テストは Rust の慣習に沿ってクレート内 `#[cfg(test)] mod tests` と
ワークスペースルートの `tests/` 統合テストに分割:

- **共有フィクスチャ** (`crates/gen-font/tests/common/mod.rs`) — 実 palt /
  vert / cmap データが必要なテスト用に Noto Variable のサブセットを
  `OnceLock` でキャッシュ; 全グリフ走査が必要な mutation テスト用には
  `write-fonts` の `FontBuilder` で組み立てた最小 TrueType (Noto 17000
  グリフを毎回触るのは無駄)。
- **`crates/gen-font/tests/build.rs`** — `glyph_codepoint`,
  `is_kana_or_punct`, `is_cjk_codepoint`, `is_kana_letter`,
  `get_cjk_glyphs`, `get_vert_alternates`, `apply_x_scale`,
  `strip_extreme_glyphs`, `apply_tracking`, `get_variable_palt`。
- **`crates/gen-font/tests/proportional.rs`** — `read_palt`,
  `shift_glyph_x`, `remove_prop_features`, `make_proportional` (palt
  ベイク、reduced palt scale、squeeze SB の sidebearing 計算、palt_override
  の優先、CFF 拒否、feature 削除後の LangSys index 整合性)。
- **`crates/gen-release/tests/release.rs`** — 公開配布の契約: GitHub
  アセット URL の形 (サイトのダウンロードボタンが参照)、npm パッケージ
  のレイアウト (`files` glob、`license` メタデータ、CSS エントリポイント
  の root 配置)。
- **`crates/gen-webfont/tests/build.rs`** — コードポイント範囲のマージ、
  unicode-range のフォーマット (5 桁含む)、JIS 区 → コードポイント、
  サブセット計画の配置 / 非重複 / 完全カバレッジ、Google-Japanese
  ストラテジーパーサーのエッジケース (コメント内 `}` を含む)。

| ファイル | テスト数 | 検証内容 |
|---|---|---|
| `gen-font/tests/build.rs` | 55 | グリフ名パース、kana / CJK 分類、GSUB 走査、x-scale、bbox 除去、tracking |
| `gen-font/tests/proportional.rs` | 19 | palt 抽出、グリフ平行移動、GPOS feature 削除、3 バケット方針 |
| `gen-release/tests/release.rs` | 2 | GitHub アセット URL 契約、npm パッケージレイアウト (files glob、license、CSS root 配置) |
| `gen-webfont/tests/build.rs` | 42 | 範囲マージ / 重複除去、5 桁 hex 含む unicode-range、JIS 区マッピング、サブセット計画の配置 / 非重複 / 完全カバレッジ、ストラテジーパーサーのエッジケース |

## コマンド

| コマンド | 用途 |
|---|---|
| `just font` | 全ファミリー × 全ウェイトの TTF を生成 |
| `just webfont` | unicode-range サブセットを生成 (`font` 依存) |
| `just release` | GitHub zip + npm + Pages パッケージ生成 (`webfont` 依存) |
| `just webfont-benchmark` | スライス方式の throttled fetch ベンチ |
| `just npm-pack` | npm パッケージのドライラン検査 |
| `just npm-publish` | npm に publish |
| `just site` | デモサイトのビルド (`site/dist/` がそのまま GitHub Pages artifact) |
| `just serve` | サイトのローカル Vite 開発サーバー |
| `just clean` | `dist/` と `site/dist/` を削除 |
| `cargo run -p gen-font -- [family] [weight ...]` | 部分ビルド (例: `normal Regular`) |
| `cargo test` | テスト実行 |

CI: `.github/workflows/pages.yml` が `main` への push ごとにデモサイトを
GitHub Pages にデプロイ。リリースパッケージングはローカル実行に統一 —
詳細は `crates/gen-release/README.md` の `just release` + `gh release upload`
の手順を参照。

## 依存

### Rust crates

- `gen_font::baker` (font-baker の Rust 移植、ワークスペース内クレート) —
  コンポジットフォントマージエンジン。`metadata_mode` で base / sub の
  identity を継承する。Stage 1 (bake) と Stage 3 (merge) を駆動。
  `sub_font.exclude_codepoints` と glyph-name collision rename をサポート
  し、merge 段で日本語慣習記号を Noto に残すために利用している。rename /
  duplicate されたグリフに対して縦書き metrics (`vmtx` / `VORG`) と
  `vert` / `vrt2` GSUB マッピングを base から継承するため、上書き対象
  の縦書き位置が崩れない。
- `skrifa` — フォントの読み取り・instancer・グリフメトリクス取得。
  variable から static への instantiation、palt / cmap / GSUB の参照に
  使う。
- `read-fonts` — 低レベルのテーブルパース。`skrifa` の下回りで、
  GPOS / GSUB の構造を直接歩くケースに利用。
- `write-fonts` — フォント書き出し・テーブル編集。subset / merge / hmtx
  書き戻し / feature record 編集に使う。
- `brotli` — WOFF2 圧縮 (`woff2` crate 経由の transitive、および manifest
  の brotli サイズ計測)。
- `woff2` — TTF → WOFF2 変換。
- `clap` — コマンドラインパーサー (`gen-font` / `gen-webfont` /
  `gen-release` バイナリ)。
- 標準の `cargo test` — テストランナー。

### Node.js (サイトのみ)

- `vite` — ビルドツール / 開発サーバー。
- サイトは webfont のソースには依存しない — jsDelivr 経由で公開済み npm
  パッケージを読み込む。

## このドキュメントの保守

以下のいずれかに変更が入った場合、本ファイル (および `ARCHITECTURE.md`) を
更新する:

- ビルドパイプラインの形 (ステージ境界、出力ファイル、中間生成物)
- プロポーショナル化 / トラッキング / bbox 除去の方針
- Webfont サブセット化のストラテジーや出力レイアウト
- リリースパッケージング表面 (zip 名、npm パッケージ形、manifest フィールド)
- テスト基盤 (フィクスチャ、ファイル分割)
- vendor 依存や CI ワークフロー

ドキュメントとコードを同期させるのは変更のフォローアップではなく、
変更そのものの一部とする。
