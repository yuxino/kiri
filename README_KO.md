<div align="center">
  <img src="src-tauri/icons/128x128.png" width="112" alt="Kiri">
  <h1>Kiri</h1>
  <p>스크린샷, 문자 인식, 화면 녹화. 캡처한 자료는 컴퓨터에 저장됩니다.</p>
</div>

[简体中文](README.md) · [English](README_EN.md) · [繁體中文](README_ZH_TW.md) · [日本語](README_JA.md) · [Deutsch](README_DE.md) · **한국어** · [Français](README_FR.md)

[공식 웹사이트](https://kiri.yuxino.cn) · [최신 버전 다운로드](https://github.com/yuxino/kiri/releases/latest)

Kiri는 macOS, Windows, Linux용 스크린샷 및 화면 녹화 앱입니다. 스크린샷에 화살표, 텍스트, 모자이크를 추가하고 이미지 속 문자를 복사할 수 있습니다. 이미지와 동영상은 자신의 컴퓨터에 저장됩니다.

![Kiri 주석 인터페이스](docs/assets/readme-preview.png)

## 기능

- 창이나 영역을 캡처하고 자르기, 그리기, 텍스트, 모자이크를 추가합니다. 스크린샷을 떠 있는 참고 이미지로 고정할 수도 있습니다.
- 로컬 OCR로 이미지 속 문자를 복사하고 QR 코드를 읽습니다.
- 시스템 소리와 마이크를 선택하여 MP4로 녹화하거나 소리 없는 GIF를 만듭니다. 저장한 동영상을 GIF로 변환할 때 먼저 동영상을 확인하며 확인과 변환 중에 취소할 수 있습니다. macOS에서는 프레임별 진행 상황과 실패 원인을 보여 줍니다.
- 한 동영상의 클립을 자르고 순서를 바꾼 뒤 새 MP4로 내보냅니다.
- 로컬 자료를 검색하고 태그, 즐겨찾기를 지정하거나 내보냅니다. 잘못 삭제한 자료는 휴지통에서 복원할 수 있습니다.

기존 주석을 선택하면 스타일을 변경할 수 있습니다. 모자이크는 자유 그리기, 사각형, 타원을 지원하며 픽셀 또는 흐림 효과와 강도를 조절할 수 있습니다. 워터마크(W)를 선택해 이미지에 반복 표시할 글자를 직접 입력하고 불투명도, 각도, 간격을 조절하세요. 도구를 다시 선택하면 기존 내용을 이어서 편집할 수 있으며 저장한 뒤에도 변경할 수 있습니다.

## 시작하기

1. macOS에서 `⇧⌘A`, Windows / Linux X11에서 `Shift+Ctrl+A`를 누른 뒤 창을 선택하거나 영역을 드래그합니다.
2. 스크린샷, 녹화 또는 OCR을 선택합니다.
3. `Enter`를 누르거나 선택 영역의 표시 없는 부분을 두 번 클릭하면 스크린샷이 완료됩니다. `Esc`로 취소합니다. 주석을 그리는 중에는 먼저 선택 도구로 바꾸세요. 텍스트를 두 번 클릭하면 계속 편집됩니다. 스크린샷은 클립보드에 복사되고 라이브러리에 저장됩니다. 도구 모음의 핀 버튼을 누르면 한 번에 저장하고 고정합니다. 완료 카드의 ‘고정’도 사용할 수 있습니다. 테두리 없는 이미지를 드래그해 이동하고 오른쪽 아래 모서리에서 비율을 유지하며 크기를 바꿀 수 있습니다. 마우스를 올리면 고정 해제와 닫기 버튼이 나타납니다.

스크린샷 모드에서 마우스를 올리면 원본 픽셀, 좌표 및 HEX 색상 값을 확대해 볼 수 있습니다. macOS에서는 `⌘C`, 다른 환경에서는 `Ctrl+C`로 색상을 복사하며 선택 영역은 유지됩니다. 주석을 시작하면 돋보기가 숨겨집니다.

주석이나 녹화를 시작하기 전에 슬라이더 버튼을 누르면 선택 영역의 가장자리에서 너비와 높이를 픽셀 단위로 입력할 수 있습니다.

‘설정 → 일반 → 언어’에서 일곱 가지 인터페이스 언어를 선택할 수 있습니다. 선택은 모든 Kiri 창에 적용되고 다시 시작해도 유지됩니다. 처음에는 시스템 언어를 따릅니다.

자료는 로컬에 저장됩니다. 원격 OCR은 선택 사항이며 업로드할 때마다 확인합니다. Linux 설정, MP4 소리 녹음, Wayland 단축키와 플랫폼 제한은 [Linux 안내](docs/linux.md)를 참고하세요.

[설치 및 사용 안내](docs/usage.ko.md) · [동영상 편집](docs/video-editing.ko.md) · [문제 신고](https://github.com/yuxino/kiri/issues) · [기여 안내](CONTRIBUTING.md)

## 기여자

코드를 작성하고, 문제를 신고하고, Kiri를 사용하거나 공유해 주신 모든 분께 감사합니다.

[초기 Linux 지원](https://github.com/yuxino/kiri/pull/20)을 제공한 [@kerwin2046](https://github.com/kerwin2046)와 [Windows 텍스트 배율에 따른 캡처 정렬](https://github.com/yuxino/kiri/pull/61)을 수정한 [@LLLin000](https://github.com/LLLin000)에게 특별히 감사드립니다.

[모든 기여자](https://github.com/yuxino/kiri/graphs/contributors)

## 커뮤니티

Kiri를 사용하고 의견을 주거나 공유해 주신 [V2EX](https://www.v2ex.com/), [LINUX DO](https://linux.do/), [Appinn](https://www.appinn.com/), [NodeLoc](https://www.nodeloc.com/) 여러분께도 감사합니다.

[MIT](LICENSE) © 2026 yuxino
