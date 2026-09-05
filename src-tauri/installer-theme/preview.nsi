; Compile-only native UI fixture. Never installs an application or writes user data.
Unicode true
ManifestDPIAware true
ManifestDPIAwareness PerMonitorV2
RequestExecutionLevel user
SetCompressor /SOLID lzma
Name "yuxino theme preview"
Caption "yuxino theme preview - no application will be installed"
OutFile "preview-only-setup.exe"
InstallDir "$TEMP\yuxino-theme-preview"
!include "MUI2.nsh"
!include "${__FILEDIR__}\theme.nsh"
!define MUI_HEADERIMAGE
!define MUI_HEADERIMAGE_BITMAP "${__FILEDIR__}\generated\header.bmp"
!define MUI_WELCOMEFINISHPAGE_BITMAP "${__FILEDIR__}\generated\sidebar.bmp"
!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_LANGUAGE "English"
!insertmacro MUI_LANGUAGE "SimpChinese"
!insertmacro MUI_LANGUAGE "Japanese"
Section "Preview only"
  Nop
SectionEnd
