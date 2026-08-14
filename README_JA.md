<div align="center">
  <img src="src-tauri/icons/128x128.png" width="112" alt="kiri アプリアイコン">
  <h1>kiri</h1>
  <p>macOS と Windows 対応の、高速で完全ローカルのキャプチャワークスペース。</p>
  <p>
    <a href="README.md">English</a>
    · <a href="README_ZH.md">简体中文</a>
  </p>
</div>

`kiri` の名前は、日本語の「切り取り」に由来します。

スクリーンショットの撮影、注釈、文字認識、範囲録画を行い、すべてをローカルライブラリに保存します。クラウドは不要です。

## 画面プレビュー

![Kiri ライブラリ](docs/screenshots/library.png)

## 機能

- **スクリーンショット** — ウインドウのクリック選択または範囲ドラッグで正確に切り取れます。
- **注釈** — ペン、図形、矢印、テキスト、モザイクに対応し、元に戻す / やり直しも可能。追加済みの注釈は再選択して編集できます。
- **OCR** — ローカルで文字認識(macOS Vision / Windows.Media.Ocr)。
- **録画** — システム音声、マイク、ポインター、クリック表示を任意で含む範囲録画。3・2・1 カウントダウン、ドラッグ可能なコントロールバー(スペースで一時停止、Esc で停止)、Retina 品質の MP4 出力。
- **GIF** — 短い録画をループ GIF に変換。
- **ライブラリ** — 日付ごとにグループ化されたキャプチャ。お気に入り、タグ、名前変更、検索、コピー、ファイル表示、復元可能なゴミ箱に対応。サイドバーとフィルターバーで種類・お気に入り・タグによる絞り込みができます。

## ダウンロード

最新ビルドは GitHub Releases からダウンロードできます。

- **macOS**:解凍して `Kiri.app` を「アプリケーション」フォルダへ移動します。グローバルショートカットには「入力監視」、キャプチャには「画面とシステム音声の録画」権限が必要です。自分で書き出す場合を除き、すべてのデータは Mac 上に留まります。
- **Windows**:インストーラを実行するだけで、追加の権限は不要です。

## ソースからビルド

Rust 1.85+、Node.js 20+、pnpm が必要です。

```bash
git clone https://github.com/yuxino/kiri.git
cd kiri
pnpm install
cargo test --manifest-path src-tauri/Cargo.toml
pnpm tauri build --no-bundle   # または ./scripts/package-app.sh でインストーラを生成
```

> 注意:素の `cargo build` で生成したバイナリは空白ウインドウになります。
> フロントエンドのアセットは `pnpm tauri build`(開発時は `pnpm tauri dev`)で
> ビルドしたときだけ埋め込まれます。

macOS のパッケージングには Xcode コマンドライン・ツールも必要です。

## ショートカット

- **⇧⌘A**(macOS)/ **Shift+Ctrl+A**(Windows)— Kiri を開く
- **Esc** — キャプチャをキャンセル
- **Return** — キャプチャをコピー
- **V** — 注釈の選択 / 移動
- **P / R / L / A / T / M** — ペン / 四角形 / 直線 / 矢印 / テキスト / モザイク
- **Delete** — 選択中の注釈を削除
- **スペース**(録画中)— 一時停止 / 再開;**Esc** — 停止
- **⌘F**(macOS)/ **Ctrl+F**(Windows)— ライブラリを検索
- **⌘Z / ⇧⌘Z**(macOS)/ **Ctrl+Z / Shift+Ctrl+Z**(Windows)— 元に戻す / やり直す

詳しくは [ROADMAP.md](ROADMAP.md) をご覧ください。

[MIT](LICENSE) © 2026 yuxino
