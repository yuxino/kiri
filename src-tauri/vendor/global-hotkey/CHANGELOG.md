# Changelog

## \[0.8.0]

- [`d706220`](https://www.github.com/tauri-apps/global-hotkey/commit/d70622077a2acef52c2359b8b65218523be72c26) ([#165](https://www.github.com/tauri-apps/global-hotkey/pull/165) by [@SergioRibera](https://www.github.com/tauri-apps/global-hotkey/../../SergioRibera)) Increased MSRV to `1.77`.
- [`5c3d182`](https://www.github.com/tauri-apps/global-hotkey/commit/5c3d182048f02cad759b4e0cb224d6fbc3952c77) ([#178](https://www.github.com/tauri-apps/global-hotkey/pull/178) by [@amrbashir](https://www.github.com/tauri-apps/global-hotkey/../../amrbashir)) On Windows, sleep 50ms in the release-detection loop to avoid burning a CPU core for the whole hold duration (e.g. push-to-talk).
- [`23b71e1`](https://www.github.com/tauri-apps/global-hotkey/commit/23b71e1b08f0e6a23e248a51fa0437ba9756366b) ([#170](https://www.github.com/tauri-apps/global-hotkey/pull/170) by [@PiasekDev](https://www.github.com/tauri-apps/global-hotkey/../../PiasekDev)) Add X11 keysym mappings for F13-F24.

## \[0.7.0]

- [`77dbe4e`](https://www.github.com/tauri-apps/global-hotkey/commit/77dbe4ebe5911f9ee41f3264ecb11295d7e6abe7) ([#150](https://www.github.com/tauri-apps/global-hotkey/pull/150) by [@Exidex](https://www.github.com/tauri-apps/global-hotkey/../../Exidex)) Use `x11rb` crate instead of `x11-dl` for linux (x11) backend.

## \[0.6.4]

- [`a25c485`](https://www.github.com/tauri-apps/global-hotkey/commit/a25c485b6fce488799510c7f70563db3ebcdb72f) ([#120](https://www.github.com/tauri-apps/global-hotkey/pull/120) by [@FabianLars](https://www.github.com/tauri-apps/global-hotkey/../../FabianLars)) Update `objc2` to `0.6`. This raises the MSRV to 1.71 which is now also set in `rust-version`.

## \[0.6.3]

- [`ddf5515`](https://www.github.com/tauri-apps/global-hotkey/commit/ddf5515712f85e887e715bda7da40becc9159ac9) ([#112](https://www.github.com/tauri-apps/global-hotkey/pull/112) by [@amrbashir](https://www.github.com/tauri-apps/global-hotkey/../../amrbashir)) Support using `Pause` or `PauseBreak` key on Windows and Linux.

## \[0.6.2]

- [`2c7397b`](https://www.github.com/tauri-apps/global-hotkey/commit/2c7397b27ccb2efd4589bb364e611a80635413c8) ([#106](https://www.github.com/tauri-apps/global-hotkey/pull/106) by [@FabianLars](https://www.github.com/tauri-apps/global-hotkey/../../FabianLars)) Fixed an issue causing compilation to fail for 32-bit targets.

## \[0.6.1]

- [`7d15d09`](https://www.github.com/tauri-apps/global-hotkey/commit/7d15d09e518130bf0a1b44e3512cb6f5ed361164) ([#99](https://www.github.com/tauri-apps/global-hotkey/pull/99) by [@madsmtm](https://www.github.com/tauri-apps/global-hotkey/../../madsmtm)) Use `objc2` internally, leading to slightly better memory- and type-safety.

## \[0.6.0]

- [`8b13a61`](https://www.github.com/tauri-apps/global-hotkey/commit/8b13a6159d776a6a282ad7ca5c4b896cc91e325a) Removed `Sync` and `Send` implementation for `GlobalHotKeyManager`
- [`8b13a61`](https://www.github.com/tauri-apps/global-hotkey/commit/8b13a6159d776a6a282ad7ca5c4b896cc91e325a) Update `windows-sys` crate to `0.59`

## \[0.5.5]

- [`c750004`](https://www.github.com/tauri-apps/global-hotkey/commit/c7500047fb62154cf861878efb334c61bd98988a) ([#92](https://www.github.com/tauri-apps/global-hotkey/pull/92) by [@IAmJSD](https://www.github.com/tauri-apps/global-hotkey/../../IAmJSD)) Fix a panic when parsing `HotKey` from a string and return an error instead, if the hotkey string consists of only modifiers and doesn't contain a key.

## \[0.5.4]

- [`e9d263c`](https://www.github.com/tauri-apps/global-hotkey/commit/e9d263c2d9b9535af8d64c7b8950308d16b57b94) Fix parsing of `MEDIATRACKPREV` and `MEDIATRACKPREVIOUS` keys.

## \[0.5.3]

- [`a468ede`](https://www.github.com/tauri-apps/global-hotkey/commit/a468ede66aa2102f146bebd71ad618eff550997a)([#75](https://www.github.com/tauri-apps/global-hotkey/pull/75)) Add `serde` feature flag and implement `Deserialize` and `Serialize` for `GlobalHotKeyEvent`, `HotKeyState` and `HotKey` types.
- [`a468ede`](https://www.github.com/tauri-apps/global-hotkey/commit/a468ede66aa2102f146bebd71ad618eff550997a)([#75](https://www.github.com/tauri-apps/global-hotkey/pull/75)) Add `HotKey::into_string` method and implement `Display` for `HotKey`.

## \[0.5.2]

- [`c530be0`](https://www.github.com/tauri-apps/global-hotkey/commit/c530be0dbf939d2dd8d05eacc2071f493769a834)([#71](https://www.github.com/tauri-apps/global-hotkey/pull/71)) Support registering media play/pause/stop/next/prev keys.
- [`24f41b0`](https://www.github.com/tauri-apps/global-hotkey/commit/24f41b0fd9f54e822e6397bc95d9e717c67aab72)([#73](https://www.github.com/tauri-apps/global-hotkey/pull/73)) Always service all pending events to avoid a queue of events from building up.

## \[0.5.1]

- [`89199d9`](https://www.github.com/tauri-apps/global-hotkey/commit/89199d930db3a71f1e19a29d6c1d6ff2e8cffb11)([#64](https://www.github.com/tauri-apps/global-hotkey/pull/64)) Add no-op implementations for unsupported targets.

## \[0.5.0]

- [`7d99bd7`](https://www.github.com/tauri-apps/global-hotkey/commit/7d99bd78a383e11ae6bb8fce0525afcc9e427c8f)([#61](https://www.github.com/tauri-apps/global-hotkey/pull/61)) Refactored the errors when parsing accelerator from string:

  - Added `HotKeyParseError` error enum.
  - Removed `Error::UnrecognizedHotKeyCode` enum variant
  - Removed `Error::EmptyHotKeyToken` enum variant
  - Removed `Error::UnexpectedHotKeyFormat` enum variant
  - Changed `Error::HotKeyParseError` inner value from `String` to the newly added `HotKeyParseError` enum.
- [`7d99bd7`](https://www.github.com/tauri-apps/global-hotkey/commit/7d99bd78a383e11ae6bb8fce0525afcc9e427c8f)([#61](https://www.github.com/tauri-apps/global-hotkey/pull/61)) Avoid panicing when parsing an invalid `HotKey` from a string such as `SHIFT+SHIFT` and return an error instead.

## \[0.4.2]

- [`b538534`](https://www.github.com/tauri-apps/global-hotkey/commit/b538534f9181ccd38e76d93368378fc6ed3a3a08) Changed window class name used interally so it wouldn't conflict with `tray-icon` crate implementation.

## \[0.4.1]

- [`1f9be3e`](https://www.github.com/tauri-apps/global-hotkey/commit/1f9be3e0631817a9c96a4d98289158286cb689e8)([#47](https://www.github.com/tauri-apps/global-hotkey/pull/47)) Add support for `Code::Backquote` on Linux.
- [`1f9be3e`](https://www.github.com/tauri-apps/global-hotkey/commit/1f9be3e0631817a9c96a4d98289158286cb689e8)([#47](https://www.github.com/tauri-apps/global-hotkey/pull/47)) On Linux, fix hotkey `press/release` events order and sometimes missing `release` event when the modifiers have been already released before the key itself has been released.
- [`1f9be3e`](https://www.github.com/tauri-apps/global-hotkey/commit/1f9be3e0631817a9c96a4d98289158286cb689e8)([#47](https://www.github.com/tauri-apps/global-hotkey/pull/47)) On Linux, improve the performance of `GlobalHotKeyManager::register_all` and `GlobalHotKeyManager::unregister_all` to 2711x faster.

## \[0.4.0]

- [`53961a1`](https://www.github.com/tauri-apps/global-hotkey/commit/53961a1ade623bb97ce96db71fbe1193ffc9d6a7)([#35](https://www.github.com/tauri-apps/global-hotkey/pull/35)) Support Pressed and Released stats of the hotkey, you can check the newly added `state` field or using the `state()` method on the `GlobalHotKeyEvent`.

## \[0.3.0]

- [`fa47029`](https://www.github.com/tauri-apps/global-hotkey/commit/fa47029435ed953b07f5809d9e521bcd2c24bf54) Update `keyboard-types` to `0.7`

## \[0.2.4]

- [`b0975f9`](https://www.github.com/tauri-apps/global-hotkey/commit/b0975f9983aa023df3cd72bbd8d3158165e9f6eb) Export `CMD_OR_CTRL` const.
- [`dc9e619`](https://www.github.com/tauri-apps/global-hotkey/commit/dc9e6197362164ef6b8aae90df41a6a2b459f5fb) Add `GlobalHotKeyEvent::id` method.
- [`b960609`](https://www.github.com/tauri-apps/global-hotkey/commit/b96060952daf8959939f07c968b8bd58e33f4abd) Impl `TryFrom<&str>` and `TryFrom<String>` for `HotKey`.

## \[0.2.3]

- [`589ecd9`](https://www.github.com/tauri-apps/global-hotkey/commit/589ecd9afd79aab93b25b357b4c70afdf69f9f6d)([#25](https://www.github.com/tauri-apps/global-hotkey/pull/25)) Fix `GlobalHotKeyManager::unregister_all` actually registering the hotkeys instead of unregistering.

## \[0.2.2]

- [`bbd3ffb`](https://www.github.com/tauri-apps/global-hotkey/commit/bbd3ffbea2a76eaae7cd344a019a942456f94a26)([#23](https://www.github.com/tauri-apps/global-hotkey/pull/23)) Generate a hash-based id for hotkeys. Previously each hotkey had a unique id which is not necessary given that only one hotkey with the same combination can be used at a time.

## \[0.2.1]

- [`b503530`](https://www.github.com/tauri-apps/global-hotkey/commit/b503530eb49a7fe8da3e49080e3f72f82a70b7a2)([#20](https://www.github.com/tauri-apps/global-hotkey/pull/20)) Make `GlobalHotKeyManager` Send + Sync on macOS.

## \[0.2.0]

- Support more variants for `HotKey::from_str` and support case-insensitive htokey.
  - [25cbda5](https://www.github.com/tauri-apps/global-hotkey/commit/25cbda58c503b8230af00c6192e87d5ce1fc2742) feat: add more variants and case-insensitive hotkey parsing ([#19](https://www.github.com/tauri-apps/global-hotkey/pull/19)) on 2023-04-19

## \[0.1.2]

- On Windows, fix registering htokeys failing all the time.
  - [65d1f6d](https://www.github.com/tauri-apps/global-hotkey/commit/65d1f6dffd54bafe46d1ae776639b5dd10e78b96) fix(window): correctly check error result on 2023-02-13
- Fix crash on wayland, and emit a warning instead.
  - [4c08d82](https://www.github.com/tauri-apps/global-hotkey/commit/4c08d82fa4a20c82988b49f718688ec29de8a781) fix: emit error on non x11 window systems on 2023-02-13

## \[0.1.1]

- Update docs
  - [6409e5d](https://www.github.com/tauri-apps/global-hotkey/commit/6409e5dd351e1cae808c0042f4507e9afad70a05) docs: update docs on 2023-02-08

## \[0.1.0]

- Initial Release.
  - [72873f6](https://www.github.com/tauri-apps/global-hotkey/commit/72873f629b47565888d5f2a4264476c9974686b6) chore: add initial release change file on 2023-01-16
  - [d0f1d9c](https://www.github.com/tauri-apps/global-hotkey/commit/d0f1d9c58eba60015f658f7a742c200c2d1bd55e) chore: adjust change file on 2023-01-16
