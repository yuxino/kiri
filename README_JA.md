<div align="center">
  <img src="src-tauri/icons/128x128.png" width="112" alt="Kiri">
  <h1>Kiri</h1>
  <p>スクリーンショット、文字認識、画面録画。データはこのコンピューターに保存されます。</p>
</div>

[简体中文](README.md) · [English](README_EN.md) · [繁體中文](README_ZH_TW.md) · **日本語** · [Deutsch](README_DE.md) · [한국어](README_KO.md) · [Français](README_FR.md)

[公式サイト](https://kiri.yuxino.cn) · [最新版をダウンロード](https://github.com/yuxino/kiri/releases/latest)

Kiri は macOS、Windows、Linux 向けのスクリーンショット・画面録画アプリです。画像に矢印や文字、モザイクを加え、画像内の文字をコピーできます。画像も動画も自分のコンピューターに保存されます。

![Kiri の注釈画面](docs/assets/readme-preview.png)

## 機能

- ウィンドウや範囲を撮影し、切り抜き、描画、文字、モザイクを追加。スクリーンショットを最前面に固定して参照することもできます。
- ローカル OCR で画像の文字をコピーし、QR コードを認識。
- システム音声やマイクを含む MP4 録画、または音声のない GIF を作成。保存済み動画の GIF 変換は先に動画を確認し、確認中や変換中にキャンセルできます。macOS ではフレームごとの進捗と失敗の理由を表示します。
- 一つの動画のクリップを切り取り、並べ替えて、新しい MP4 として書き出し。
- ローカルデータの検索、タグ付け、お気に入り登録、書き出し。削除したデータはゴミ箱から復元できます。

既存の注釈を選択すると、そのスタイルを変更できます。モザイクにはフリーハンド、長方形、楕円があり、ピクセル化またはぼかしと強さを選べます。透かし（W）は画像上で繰り返し表示する文字を直接入力し、不透明度、角度、間隔を調整できます。ツールを再度選ぶと既存の内容を続けて編集でき、保存後も変更できます。

## はじめに

1. macOS では `⇧⌘A`、Windows / Linux X11 では `Shift+Ctrl+A` を押し、ウィンドウを選ぶか範囲をドラッグします。
2. スクリーンショット、録画、OCR を選びます。
3. `Enter`、または選択範囲内の何もない場所をダブルクリックすると撮影が完了します。`Esc` でキャンセル。注釈中は先に選択ツールに戻してください。文字のダブルクリックは編集に使います。スクリーンショットはクリップボードとライブラリに保存されます。ツールバーのピンを押すと、保存と最前面への固定が一度にできます。完了カードの「固定」も使えます。参照画像は枠なしで、ドラッグで移動し、右下隅で縦横比を保ってサイズを変更できます。マウスを重ねると固定解除と閉じるボタンが表示されます。

スクリーンショットモードでは、マウスを重ねると元のピクセル、座標、HEX 色値を拡大表示します。macOS は `⌘C`、ほかの環境は `Ctrl+C` で色をコピーできます。選択範囲は保持され、注釈を始めると拡大鏡は非表示になります。

注釈や録画の前にスライダーボタンを押すと、選択範囲の横で幅と高さをピクセル単位で入力できます。

「設定 → 一般 → 言語」で七つの表示言語から選べます。選択はすべての Kiri ウィンドウに反映され、再起動後も保持されます。初回はシステム言語に従います。

データはローカルに保存されます。リモート OCR は任意で、アップロードの前に毎回確認します。Linux の設定、MP4 音声録音、Wayland のショートカット、制限は [Linux ガイド](docs/linux.md)をご覧ください。

[インストールと使い方](docs/usage.ja.md) · [動画編集](docs/video-editing.ja.md) · [問題を報告](https://github.com/yuxino/kiri/issues) · [貢献ガイド](CONTRIBUTING.md)

## 貢献者

コードを書く方、不具合を報告する方、試用や共有をしてくださる皆さんに感謝します。

[Linux の初期対応](https://github.com/yuxino/kiri/pull/20)を提供した [@kerwin2046](https://github.com/kerwin2046)、[Windows の文字拡大時の撮影位置](https://github.com/yuxino/kiri/pull/61)を修正した [@LLLin000](https://github.com/LLLin000) に特に感謝します。

[すべての貢献者](https://github.com/yuxino/kiri/graphs/contributors)

## コミュニティ

試用、フィードバック、共有をしてくださった [V2EX](https://www.v2ex.com/)、[LINUX DO](https://linux.do/)、[Appinn](https://www.appinn.com/)、[NodeLoc](https://www.nodeloc.com/) の皆さんにも感謝します。

[MIT](LICENSE) © 2026 yuxino
