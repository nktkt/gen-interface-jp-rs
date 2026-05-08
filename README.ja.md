# Gen Interface JP (Rust 移植版)

<p><strong><a href="https://github.com/yamatoiizuka/gen-interface-jp/blob/main/README.md">English</a></strong> | 日本語</p>

Gen Interface JP は、デジタルインターフェースのために設計された、欧文と和文の調和を目指す書体です。  
明快な UI 用書体である Inter に Noto Sans JP の和文グリフを合わせ、多言語環境で一貫した読みやすさを実現します。

本リポジトリは、オリジナルの Python 実装を **Rust へ移植**したビルドパイプラインです。生成されるフォント本体および公開仕様はオリジナルと等価です。

## Overview

### 2 Families

- **Gen Interface JP**: 汎用／本文用
- **Gen Interface JP Display**: 見出し用

### 8 Weights

- 100: Thin
- 200: ExtraLight
- 300: Light
- 400: Regular
- 500: Medium
- 600: SemiBold
- 700: Bold
- 800: ExtraBold

### Web Fonts

Web プロジェクトにおいて、head 内のスタイルシートの読み込みのみで Web フォントを使用できます。  
[Google Fonts と同様のサブセット化](https://developers.googleblog.com/google-fonts-launches-japanese-support/)により、単一フォントデータと比べ高速な表示を実現しています。

#### Gen Interface JP

```html
<!-- 
 index.html 
 100.css ... 800.css
 -->
<head>
  <link
    rel="stylesheet"
    href="https://cdn.jsdelivr.net/npm/gen-interface-jp@latest/400.css"
  />
</head>
```

```css
/* style.css */
body {
  font-family: "Gen Interface JP", sans-serif;
  font-weight: 400; /* 100–800 */
}
```

#### Gen Interface JP Display

```html
<!-- 
 index.html 
 display-100.css ... display-800.css
 -->
<head>
  <link
    rel="stylesheet"
    href="https://cdn.jsdelivr.net/npm/gen-interface-jp@latest/display-800.css"
  />
</head>
```

```css
/* style.css */
h1,
h2 {
  font-family: "Gen Interface JP Display", sans-serif;
  font-weight: 800; /* 100–800 */
}
```

### CSS List

- `all.css`: 全 16 ウェイトの CSS
- `400.css`: Gen Interface JP Regular (400) の CSS
- `display-400.css`: Gen Interface JP Display Regular (400) の CSS

## Repository

```text
crates/
  gen-font/       # コアフォント生成 (旧 src/font)
  gen-webfont/    # ウェブフォント CSS + サブセット WOFF2 (旧 src/webfont)
  gen-release/    # GitHub Release / npm パッケージング (旧 src/release)
docs/
  ARCHITECTURE.md
```

このリポジトリの主成果物は `crates/gen-font/` で生成するフォントファミリーです。`crates/gen-webfont/` と `crates/gen-release/` は、その生成物から派生する配信・公開用の工程です。生成物は `dist/` 配下に置かれ、リポジトリにはコミットしません。

ビルドパイプラインや内部仕様の詳細は [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) を参照してください。

## Quick Start

```bash
just font       # dist/ttf/ にフォントを生成 (cargo run -p gen-font)
just webfont    # dist/webfont/ に CSS + サブセット WOFF2 を生成 (cargo run -p gen-webfont)
just release    # dist/release/ に配布用パッケージを生成 (cargo run -p gen-release)
```

`just` を導入していない場合は、対応する `cargo run -p <crate>` を直接実行できます。webfont サブセット化、リリース梱包、テスト、npm 公開などの全コマンドは [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) の「コマンド」を参照してください。

## License

このリポジトリのソースコードは [MIT License](LICENSE)、生成されたフォント本体は [SIL Open Font License 1.1](https://scripts.sil.org/OFL) です。　　

`vendor/` 配下は、それぞれに同梱のライセンスに従います。

## References

- [Noto Sans JP](https://github.com/notofonts/noto-cjk)
- [Inter](https://github.com/rsms/inter)
