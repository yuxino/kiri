<div align="center">
  <img src="src-tauri/icons/128x128.png" width="112" alt="kiri アプリアイコン">
  <h1>Kiri</h1>
  <p>ローカル優先のスクリーンショット、注釈、OCR、範囲録画ツール。</p>
  <p>
    <a href="README.md">简体中文</a>
    · <a href="README_EN.md">English</a>
    · <strong>日本語</strong>
  </p>
</div>

Kiri は macOS と Windows に対応しています。macOS では `⇧⌘A`、Windows では `Shift+Ctrl+A` を押し、ウインドウまたは範囲を選択して、キャプチャ、注釈、文字認識、録画を行えます。スクリーンショットはクリップボードへコピーされ、画像、MP4、GIF はローカルライブラリに保存されます。

## 機能

- **スクリーンショットと注釈**：ウインドウをクリックするか範囲をドラッグし、トリミング、ペン、図形、矢印、テキスト、モザイク、元に戻す / やり直すを使用できます。現在のリリースで作成した注釈は、完了カードまたはライブラリから再編集できます。
- **OCR** — 既定では macOS Vision または Windows.Media.Ocr でローカル認識します。任意のリモート OCR は送信のたびに確認します。
- **録画と GIF**：システム音声、マイク、ポインター、クリック表示を任意で含む範囲録画を MP4 または GIF で保存します。
- **ローカルライブラリ**：検索、お気に入り、タグ、名前変更、復元可能なゴミ箱に対応します。別のローカルフォルダや外付けディスクも保存先にできます。

## ダウンロード

公開ビルドは [GitHub Releases](https://github.com/yuxino/kiri/releases) に掲載されます。macOS の安定版と Windows 候補版では公開状況が異なる場合があります。

- **macOS 14 以降**：Apple Silicon と Intel に対応する Universal `.dmg` をダウンロードし、`Kiri.app` を「アプリケーション」へドラッグします。キャプチャには「画面とシステム音声の録画」が必要で、「入力監視」はクリック表示を使う場合のみ必要です。マイク録音には macOS 15 以降が必要です。
- **Windows 11 (x64)**：現在のソースで対応しています。v1.4.8 インストーラーはネイティブキャプチャの実機検証中のドラフト候補で、まだ正式公開されていません。`.exe` インストーラーを実行します。画面キャプチャに追加権限は不要で、マイクは Windows のプライバシー設定に従います。インストーラーは Authenticode 署名されていないため、SmartScreen が警告を表示する場合があります。

macOS 版はプロジェクトが管理するローカルの自己署名 ID を使用し、Developer ID 署名や Apple 公証は行っていません。初回起動がブロックされた場合は、Control クリックから「開く」、または「プライバシーとセキュリティ」の「このまま開く」を使用してください。

## プライバシー

キャプチャ、ローカル OCR、エンコードは既定で端末内に留まります。リモート OCR は任意で、API キーは macOS キーチェーンまたは Windows 資格情報マネージャーに保存されます。各リクエストには明示的な「送信」または「再試行」が必要です。

再編集可能なスクリーンショットは注釈前の元画像もローカルに保存するため、モザイクや図形で隠した画素が残る場合があります。トリミングを保存すると範囲外の画素も削除されます。Windows の MP4 録画と GIF 生成にはシステムのメディア機能と内蔵エンコーダーを使用するため、FFmpeg はダウンロードしません。macOS の録画と GIF 変換は引き続き FFmpeg を使用し、必要な場合だけダウンロードしてキャッシュします。エンコードはローカルで行います。

## ソースからビルド

Rust 1.88+、Node.js 20.19+（または 22.12+）、pnpm が必要です。macOS では Xcode Command Line Tools、Windows では MSVC C++ Build Tools も必要です。

```bash
git clone https://github.com/yuxino/kiri.git
cd kiri
pnpm install
pnpm tauri dev
pnpm tauri build --no-bundle
```

macOS の開発ビルドには安定した署名 ID も必要です。通常の `cargo build` で生成した実行ファイルにはフロントエンド資産が含まれないため、Tauri コマンドを使用してください。

## ショートカット

- **⇧⌘A** (macOS) / **Shift+Ctrl+A** (Windows) — Kiri を開く
- **Esc** — キャプチャをキャンセル、録画中は停止
- **Return** — スクリーンショットを確定
- **C** — スクリーンショット編集時のトリミング
- **⌘F** (macOS) / **Ctrl+F** (Windows) — ライブラリを検索
- **⌘Z / ⇧⌘Z** (macOS) / **Ctrl+Z / Shift+Ctrl+Z** (Windows) — 元に戻す / やり直す

[プライバシー](PRIVACY.md)、[ロードマップ](ROADMAP.md)、[コントリビューションガイド](CONTRIBUTING.md)、[セキュリティポリシー](SECURITY.md)、[ドキュメント一覧](docs/README.md) も参照してください。

[MIT](LICENSE) © 2026 yuxino
