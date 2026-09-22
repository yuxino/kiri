# Wry Windows teardown backport

`wry/` contains the crates.io Wry 0.55.1 distribution with one
Windows-only source change in `src/webview2/mod.rs`: remove the parent-window subclass
before releasing its boxed WebView2 controller. Releasing that controller can
dispatch nested window messages; retaining the subclass during release lets
those messages access cleared or freed controller data.

The change is backported from the merged upstream fix:

- https://github.com/tauri-apps/wry/pull/1795
- https://github.com/tauri-apps/wry/commit/3fbf592feab29ffb269778a6f5573746a2f25a4a

Original crate SHA-256:
`186f9871daa55fd9c016578b810d149de58367113db7fb72b462d2323ce19514`.
The upstream Apache-2.0 and MIT licenses remain in `wry/`.
One trailing space in upstream `SECURITY.md` is removed for repository diff checks.

Tauri runtime 2.11.4 constrains Wry to the 0.55 series. The upstream fix shipped
in 0.56.1, so a normal dependency update cannot select it for this runtime.
Remove this directory and the Cargo patch when Kiri adopts a compatible Tauri
release that includes the fix. Do not edit the shared Cargo registry cache.

Windows desktop acceptance repeatedly cancels a real countdown by both click
and Escape, then completes recording, pause, resume and stop. Failed runs retain
the crash dump and matching symbols; the passing application must also complete
the existing native video export tests.

## macOS exclusive capture hotkeys

`global-hotkey/` contains the crates.io global-hotkey 0.8.0 distribution with
one source change in `src/platform_impl/macos/mod.rs`: pass
`kEventHotKeyExclusive` (1) to `RegisterEventHotKey`. Upstream passes zero,
which permits shared registrations and can report success even when an
exclusive registration in another process prevents the expected behavior.
Kiri needs registration failures to preserve the previous capture shortcut
when a replacement conflicts (issue #21). Windows is unchanged.

Original crate SHA-256:
`8c386b0a4a70cb2d39fffd74480f985b6f0bfbcb934b6a6b6b7e630e448f242e`.
Upstream source commit: `2a620bf3852008b568f6d36c2baedcc3dd0822f2`.

One trailing whitespace line in the upstream packaging workflow is normalized.
The original MIT and Apache-2.0 licenses are retained. Remove this patch when
the upstream library exposes an exclusive-registration option, or adopts
exclusive native registration. Do not modify the shared registry cache.

Native acceptance uses a separate Carbon process holding an exclusive binding:
restoring that default must fail while the custom shortcut remains enabled;
after the helper exits, restoring the default must succeed.
