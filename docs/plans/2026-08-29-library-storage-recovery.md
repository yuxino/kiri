# Library Storage and Recovery Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Fix false missing-video errors and let users move one managed Kiri library to a local or external disk without losing recovery behavior.

**Architecture:** Keep one active library containing `library.json`, `Assets`, `Annotations`, and rebuildable thumbnails. Replace the immutable root plus bare `AssetLibrary` state with one locked library context so metadata and paths switch together. A backend-owned location config and root marker identify the library; frontend windows receive only status labels and native actions, never arbitrary filesystem paths.

**Tech Stack:** Rust/Tauri 2, React/TypeScript, Node test runner, serde JSON, native folder/file dialogs.

---

### Task 1: Fix the viewer state machine

**Files:**
- Modify: `src/windows/ViewerWindow.tsx`
- Modify: `src/lib/ipc.ts`
- Modify: `scripts/repository-hygiene.test.mjs`

**Steps:**
1. Add a regression fixture proving an unresolved asset never renders as `<img>` or `<video>`.
2. Introduce explicit `loading`, `ready`, `missing`, `unreadable`, and `playbackFailed` states.
3. Render media only after `getAsset` succeeds; on media failure ask the backend for current availability.
4. Give playback failures Retry and Reveal actions; do not label them missing.
5. Run `pnpm test:release-tools` and `pnpm build`.

### Task 2: Make the library root and availability one state boundary

**Files:**
- Create: `src-tauri/src/core/library_location.rs`
- Modify: `src-tauri/src/core/mod.rs`
- Modify: `src-tauri/src/core/library.rs`
- Modify: `src-tauri/src/state.rs`
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/protocol.rs`
- Modify: `src-tauri/src/lib.rs`

**Steps:**
1. Add tests for safe index filenames, duplicate ids/filenames, root markers, missing custom roots, and asset availability.
2. Add `LibraryContext { root, library_id, library: Option<AssetLibrary>, availability }` behind one mutex.
3. Store the selected root in the native app config and a schema/version/library UUID marker in the root.
4. Open the default root with creation allowed; open a remembered custom root only when its marker exists and matches. Never create a missing custom root or silently fall back.
5. Validate every indexed filename as one basename and reject symlinks for reads.
6. Refactor commands and protocol handlers to resolve metadata and files through the same locked context.
7. Add DTOs/commands for library status and per-asset availability.

### Task 3: Add transactional whole-library migration

**Files:**
- Modify: `src-tauri/src/core/library_location.rs`
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src/lib/ipc.ts`

**Steps:**
1. Test migration success, cancellation, copy failure, invalid destination, marker mismatch, and unavailable-root relocation.
2. Open the native folder picker off the main thread.
3. For a new location, create a staged `Kiri Library`, copy `Assets`, `Annotations`, and the index without following symlinks, then validate counts, sizes, marker, and parseability.
4. Atomically install the staged destination and switch the config/context only after verification. Keep the source library untouched.
5. Support locating the same library after a folder or external disk path changes.
6. Add Reveal, Change Location, Retry, and Restore Default commands. Restore Default uses the same staged migration path.
7. Block capture/recording start while the active library is unavailable or migration is in progress.

### Task 4: Restore an individually missing asset

**Files:**
- Modify: `src-tauri/src/core/library.rs`
- Modify: `src-tauri/src/commands.rs`
- Modify: `src/lib/ipc.ts`
- Modify: `src/windows/ViewerWindow.tsx`
- Modify: `src/windows/LibraryWindow.tsx`

**Steps:**
1. Test guarded removal and atomic replacement of a missing asset.
2. Add a native file picker filtered by the asset kind.
3. Validate the selected regular file, copy it atomically to the managed expected path, and preserve metadata.
4. Add guarded “Remove from Library” that succeeds only while the file is still missing.
5. Show missing state and recovery actions in Viewer and Library; disable Copy/GIF/Open while unavailable.

### Task 5: Persist failed recording finalization for recovery

**Files:**
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/state.rs`
- Modify: `src/lib/ipc.ts`
- Modify: `src/windows/LibraryWindow.tsx`

**Steps:**
1. Test that a valid merged MP4 is moved from temporary storage into an internal recovery spool when library import fails.
2. Store a bounded JSON manifest next to each pending MP4.
3. Expose pending recording count and a native command to retry import after the library becomes ready.
4. Show one compact library banner with Retry; remove recovery files only after durable import.

### Task 6: Add the storage settings UI and translations

**Files:**
- Modify: `src/settings/SettingsView.tsx`
- Modify: `src/settings/settings.css`
- Modify: `src/i18n/en.json`
- Modify: `src/i18n/zh-Hans.json`
- Modify: `src/i18n/ja.json`

**Steps:**
1. Add a General settings row showing Default/selected location label and current status.
2. Add Reveal, Change, Retry, Locate Existing, and Restore Default controls as applicable.
3. Keep copy short and action-specific; omit generic storage disclaimers.
4. Keep all three dictionaries key-identical and verify narrow layouts.

### Task 7: Record the durable product decision

**Files:**
- Create: `docs/adr/0018-managed-library-location-and-recovery.md`
- Modify: `docs/README.md`
- Modify: `docs/architecture.md`
- Modify: `README.md`
- Modify: `README_ZH.md`
- Modify: `README_EN.md`
- Modify: `README_JA.md`
- Modify: `PRIVACY.md`
- Modify: `PRIVACY_ZH.md`
- Modify: `ROADMAP.md`

**Steps:**
1. Document one managed library, whole-library migration, offline behavior, and pending recording recovery.
2. Keep README copy limited to actual controls and recovery behavior.
3. State that custom storage supports local and external disks; omit cloud-storage support.
4. Update the documentation index and roadmap.

### Task 8: Verify, package, clean, and deliver

**Steps:**
1. Run focused JS tests and `pnpm build` while iterating.
2. Warn before Rust verification because it recreates several GiB under `src-tauri/target`.
3. Run Rust test/check/clippy/rustfmt, then fixed-path signed app QA for viewer loading, settings, migration cancellation, and unavailable states using only isolated test data.
4. Run `git diff --check` and repository hygiene tests.
5. Remove this completed plan from the working tree, keep it in Git history, and clean `src-tauri/target` without touching the global Cargo registry.
6. Commit, push `main`, verify local/remote parity and a clean worktree, then wait for all main CI jobs to pass.
