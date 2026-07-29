<div align="center">
  <img src="Resources/Assets/kiri-icon.png" width="112" alt="kiri">
  <h1>kiri</h1>
  <p>すばやく撮れて、あとから見つかる macOS スクリーンショット</p>
  <p>
    <strong>アーリープレビュー</strong>
    · <a href="README.md">简体中文</a>
    · <a href="README_EN.md">English</a>
  </p>
</div>

`kiri` の名前は、日本語の「切り取り」に由来します。

画面の一部を切り取り、注釈を加えてコピーできるネイティブ macOS
アプリです。完成したキャプチャはローカルライブラリにも自動保存されるため、
クリップボードを書き換えたあとでも見つけ直せます。

## 現在できること

- 排他的な **⌥⌘2**（既定）または **⌃⇧2** からショートカットを選択
- 画面を静止し、最前面のウインドウへ吸着、または自由な範囲を選択
- 8 個のハンドルで調整し、ダブルクリックまたは Return でその場で注釈
- 色で区別されたペン、四角形、矢印、テキスト、モザイク
- Return ですぐコピー、または保存、ピン留め、フルエディタを選択
- 右下の Quick Access からプレビュー、ドラッグ共有、追加編集
- 完成したキャプチャは一度だけ保存され、履歴へ自動追加
- ライブラリ内の検索、お気に入り、再コピー
- ゴミ箱への移動、復元、完全削除
- 元アプリ、画像サイズ、種類、作成日時を記録

> kiri は現在、ソースコードのアーリープレビューです。録画、GIF 書き出し、
> スクロール長尺キャプチャはロードマップに含まれていますが、まだ利用できません。

## ソースから実行

macOS 14 以降と Swift 6 が必要です。

```bash
git clone https://github.com/yuxino/kiri.git
cd kiri
swift run kiri-core-tests
./scripts/package-app.sh
open dist/kiri.app
```

ローカル署名証明書がある場合は、
`KIRI_CODESIGN_IDENTITY="証明書名" ./scripts/package-app.sh` を使うと、
ビルド後もアプリの署名 ID を安定させられます。

最初のキャプチャ時に macOS の画面収録権限が必要です。データは
`~/Library/Application Support/kiri/` に保存され、自動で外部へ送信されません。

詳しくは [ROADMAP.md](ROADMAP.md) をご覧ください。

[MIT](LICENSE) © 2026 yuxino
