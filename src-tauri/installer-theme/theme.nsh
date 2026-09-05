; yuxino installer theme v1.0.0
; Visual declarations only. Tauri continues to own installation, /P, /S,
; /UPDATE, architecture selection, WebView2, shortcuts and uninstallation.
!ifndef YUXINO_INSTALLER_THEME
!define YUXINO_INSTALLER_THEME

SetFont "Segoe UI" 9
!define MUI_BGCOLOR "FFFFFF"
!define MUI_TEXTCOLOR "403953"
!define MUI_HEADERIMAGE_RIGHT
!define MUI_HEADERIMAGE_BITMAP_STRETCH AspectFitHeight
!define MUI_WELCOMEFINISHPAGE_BITMAP_STRETCH AspectFitHeight
!define MUI_INSTFILESPAGE_COLORS "403953 F7F6FD"

; Numeric language IDs are intentional: Tauri includes this file before
; its MUI_LANGUAGE declarations. ^Name is resolved by NSIS, not hardcoded.
LangString YuxinoWelcomeTitle 1033 "Welcome to $(^Name)"
LangString YuxinoWelcomeTitle 2052 "欢迎使用 $(^Name)"
LangString YuxinoWelcomeTitle 1041 "$(^Name) へようこそ"
LangString YuxinoWelcomeText 1033 "A small tool, ready for your everyday.$\r$\n$\r$\nThis wizard will guide you through installing $(^Name).$\r$\n$\r$\nChoose Next to get started."
LangString YuxinoWelcomeText 2052 "让日常轻一点的小工具。$\r$\n$\r$\n安装向导会帮你把 $(^Name) 安装到这台电脑。$\r$\n$\r$\n点击「下一步」，我们开始吧。"
LangString YuxinoWelcomeText 1041 "毎日を、少し軽やかに。$\r$\n$\r$\nこのウィザードで $(^Name) をインストールします。$\r$\n$\r$\n「次へ」を押して始めましょう。"
LangString YuxinoFinishTitle 1033 "$(^Name) is ready"
LangString YuxinoFinishTitle 2052 "$(^Name) 安装完成"
LangString YuxinoFinishTitle 1041 "$(^Name) の準備ができました"
LangString YuxinoFinishText 1033 "Thanks for giving this little app a home.$\r$\n$\r$\nChoose Finish to close this wizard."
LangString YuxinoFinishText 2052 "谢谢你给这个小工具一个位置。$\r$\n$\r$\n点击「完成」关闭安装向导。"
LangString YuxinoFinishText 1041 "この小さなアプリを選んでくれて、ありがとう。$\r$\n$\r$\n「完了」を押してウィザードを閉じてください。"

!define MUI_WELCOMEPAGE_TITLE "$(YuxinoWelcomeTitle)"
!define MUI_WELCOMEPAGE_TEXT "$(YuxinoWelcomeText)"
!define MUI_FINISHPAGE_TITLE "$(YuxinoFinishTitle)"
!define MUI_FINISHPAGE_TEXT "$(YuxinoFinishText)"
!endif
