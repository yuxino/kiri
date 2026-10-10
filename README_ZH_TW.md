<div align="center">
  <img src="src-tauri/icons/128x128.png" width="112" alt="Kiri">
  <h1>Kiri</h1>
  <p>截圖、文字辨識與螢幕錄影，素材儲存在本機。</p>
</div>

[简体中文](README.md) · [English](README_EN.md) · **繁體中文** · [日本語](README_JA.md) · [Deutsch](README_DE.md) · [한국어](README_KO.md) · [Français](README_FR.md)

[官網](https://kiri.yuxino.cn) · [下載最新版本](https://github.com/yuxino/kiri/releases/latest)

Kiri 支援 macOS、Windows 和 Linux。截圖後可以畫箭頭、加文字、打馬賽克與擷取文字，圖片和影片都保存在自己的電腦上。

![Kiri 標註介面預覽](docs/assets/readme-preview.png)

## 功能

- 擷取視窗或區域、裁切、畫圖、加文字或馬賽克，也能將截圖置頂當作參考。
- 使用本機 OCR 複製圖片中的文字，辨識 QR Code。
- 錄製 MP4，可選系統音訊和麥克風，也能製作無聲 GIF。素材庫中的影片轉 GIF 會先檢查影片，檢查和轉換時可取消；macOS 顯示逐影格進度，失敗時保留具體原因。
- 裁切、重新排列同一部影片的片段，再匯出新的 MP4。
- 搜尋、加標籤、加入收藏和匯出本機素材，誤刪可從垃圾桶還原。

選取既有標註即可修改樣式。馬賽克支援自由畫筆、矩形和橢圓，可選像素或模糊效果並調整強度。點擊浮水印（W）後直接在圖片中輸入平鋪浮水印，調整不透明度、角度和間距；再次點擊工具即可繼續編輯既有內容，儲存後仍可修改。

## 開始使用

1. 按 `⇧⌘A`（macOS）或 `Shift+Ctrl+A`（Windows / Linux X11），點選視窗或拖曳選取區域。
2. 選擇截圖、錄影或 OCR。
3. 按 `Enter` 或連按兩下選取區域中的空白處完成截圖，`Esc` 取消。標註時請先切換到選取工具；連按兩下文字仍用於編輯。截圖會複製到剪貼簿，也會儲存至素材庫；點擊截圖工具列的圖釘可一步儲存並置頂；完成卡上的「置頂」也能開啟浮動參考圖。置頂圖沒有邊框，拖曳圖片可移動，拖曳右下角可等比例縮放，移入後可取消置頂或關閉。

截圖模式下，懸停可放大查看原始像素、座標和 HEX 色值；按 `⌘C`（macOS）或 `Ctrl+C` 複製色值，選取範圍會保留。開始標註後放大鏡隱藏。

開始標註或錄影前，點選滑桿按鈕，可在選取區域旁直接輸入像素寬度與高度。

在「設定 → 一般 → 語言」選擇七種介面語言。手動選擇會套用到每個 Kiri 視窗，重新啟動後仍會保留；首次啟動跟隨系統語言。

素材保存在本機。遠端 OCR 可選，每次上傳前都會詢問。Linux 的設定、MP4 音訊錄製、Wayland 快速鍵與平台限制見 [Linux 指南](docs/linux.md)。

[安裝、使用與常見問題](docs/usage.zh-TW.md) · [影片剪輯](docs/video-editing.zh-TW.md) · [回報問題](https://github.com/yuxino/kiri/issues) · [貢獻指南](CONTRIBUTING.md)

## 貢獻者

感謝每一位寫程式、回報問題、試用與分享的朋友。

特別感謝 [@kerwin2046](https://github.com/kerwin2046) 提供 [Linux 初始支援](https://github.com/yuxino/kiri/pull/20)，以及 [@LLLin000](https://github.com/LLLin000) 修復 [Windows 文字縮放下的截圖對齊](https://github.com/yuxino/kiri/pull/61)。

[查看所有貢獻者](https://github.com/yuxino/kiri/graphs/contributors)

## 社群致謝

也感謝 [V2EX](https://www.v2ex.com/)、[LINUX DO](https://linux.do/)、[小眾軟體](https://www.appinn.com/)與 [NodeLoc](https://www.nodeloc.com/) 社群朋友的試用、回饋與分享。

[MIT](LICENSE) © 2026 yuxino
