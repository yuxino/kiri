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

画面の一部を一回の操作でコピーし、必要なときだけ注釈を加えられるネイティブ macOS
アプリです。完成したキャプチャはローカルライブラリにも自動保存されるため、
クリップボードを書き換えたあとでも見つけ直せます。

## 現在できること

- Kiri が先に受け取る **⇧⌘A** でキャプチャを開始し、他アプリの同時動作を防止
- 画面を静止し、最前面のウインドウへ吸着、または自由な範囲を選択
- 通常のキャプチャを遅くしない、独立した「キャプチャして注釈」入口
- ペン、四角形、矢印、テキスト、モザイク、元に戻す、やり直し
- 注釈モードでは Return でコピー、または保存、ピン留め、フルエディタを選択
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

パッケージスクリプトは Apple Development、Developer ID、または kiri の安定した
ローカル証明書を優先し、画面収録権限を壊す一時署名へ暗黙に切り替えません。
`KIRI_CODESIGN_IDENTITY="証明書名"` で明示的に指定できます。
`KIRI_ALLOW_ADHOC_SIGNING=1` は権限を保持しなくてよい使い捨てビルド専用です。

最初のキャプチャ時に macOS の画面収録権限が必要です。kiri は起動ごとに
システムの権限要求を一度だけ行い、それ以降は設定を開く、または kiri を終了する
操作を表示して、同じダイアログを繰り返しません。データは
`~/Library/Application Support/kiri/` に保存され、自動で外部へ送信されません。

詳しくは [ROADMAP.md](ROADMAP.md) をご覧ください。

[MIT](LICENSE) © 2026 yuxino
