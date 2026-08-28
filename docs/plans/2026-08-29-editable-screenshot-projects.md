# Editable Screenshot Projects Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Give every image an explicit editor entry and preserve new screenshot annotations as editable local project data without changing the Swift-compatible `library.json` schema.

**Architecture:** Keep `Assets/<filename>.png` as the flattened image used by thumbnails, clipboard copy, quick preview, and export. Store editable state separately under `Annotations/<asset-id>.json` with an immutable `Annotations/<asset-id>.source.png`; only captures with annotations create the duplicate source immediately, while legacy or unannotated images create it on their first edited save. Add an explicit `open_editor` path for images while preserving `open_asset` as the Viewer path for every supported asset type.

**Tech Stack:** React 19, TypeScript, Canvas 2D, Tauri 2 IPC/custom protocol, Rust, serde, image, Node test runner.

---

### Task 1: Versioned annotation document and coordinate transforms

**Files:**
- Create: `src/annotation/project.js`
- Create: `src/annotation/project.d.ts`
- Create: `scripts/annotation-project.test.mjs`
- Modify: `package.json`
- Modify: `src/annotation/model.ts`
- Modify: `src/annotation/AnnotationCanvas.tsx`

**Step 1: Write failing pure-module tests**

Cover versioned serialization, rejection of malformed or oversized documents, and stable coordinate handling for every mark kind.

**Step 2: Run the focused test and verify failure**

Run: `node --test scripts/annotation-project.test.mjs`

Expected: FAIL because the project module does not exist.

**Step 3: Implement the minimal document module**

Use schema version 1 with fixed logical `canvas`, immutable `sourcePixels`, and ordered `marks`. Validate finite geometry and known enums before returning a document. Keep marks in the fixed document coordinate space and transform pointer/display coordinates at the canvas boundary so reopening in a differently sized editor does not rewrite or drift stored geometry.

**Step 4: Expose canvas state safely**

Initialize `AnnotationHistory` from validated marks and expose a snapshot method after pending text is committed. Keep selection and undo stacks session-local; persist only visible ordered marks.

**Step 5: Run tests and build**

Run: `node --test scripts/annotation-project.test.mjs && pnpm build`

Expected: PASS with no TypeScript warnings.

### Task 2: Backward-compatible annotation sidecars

**Files:**
- Modify: `src-tauri/src/core/library.rs`
- Test: inline tests in `src-tauri/src/core/library.rs`

**Step 1: Write failing lifecycle tests**

Cover importing an editable image, preserving byte-compatible `library.json`, creating a source lazily for a legacy image, clearing an empty project, recoverable Trash, single and batch permanent deletion, empty Trash, and cleanup-failure accounting.

**Step 2: Run focused tests and verify failure**

Run: `cargo test --locked --manifest-path src-tauri/Cargo.toml core::library::tests::editable`

Expected: FAIL because sidecar APIs do not exist.

**Step 3: Implement `Annotations` storage**

Create the directory at library open. Derive filenames only from parsed UUIDs. Keep source images immutable after creation. Treat the flattened asset as the current shareable result and the document/source pair as editor state. Do not add fields to `CaptureAsset`.

**Step 4: Integrate cleanup**

Make all permanent-delete paths remove asset, thumbnail, annotation document, and annotation source. Moving to or restoring from Trash changes only the index and remains recoverable.

**Step 5: Run focused tests**

Run: `cargo test --locked --manifest-path src-tauri/Cargo.toml core::library::tests`

Expected: PASS.

### Task 3: IPC, custom protocol, and window routing

**Files:**
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/protocol.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src/lib/ipc.ts`

**Step 1: Write failing parser and security tests**

Cover bounded staged-document parsing, PNG/document dimension agreement, editor-only project reads/updates, active-overlay-only capture confirmation, stale-stage rejection, and annotation-source protocol UUID/asset-kind checks.

**Step 2: Implement capture and editor persistence**

Before the existing raw-PNG confirm/update calls, a bounded JSON command stages the validated document and binds it to the active overlay or `editor-<asset-id>` window. Annotated capture confirmation crops the clean source from the still-live frozen capture, imports the flattened PNG with its source/document, then runs the existing copy, completion-card, teardown, and focus-restoration flow. Editor update validates the flattened PNG, writes/copies externally as requested, and persists or clears the sidecar with thumbnail invalidation.

**Step 3: Route image assets to the editor**

Add `open_editor` for images and keep `open_asset` opening `viewer-<uuid>` for quick preview. Preserve single-window reuse and explicit focus only after the user's open action.

**Step 4: Run backend checks**

Run: `cargo test --locked --manifest-path src-tauri/Cargo.toml --all-targets`

Expected: PASS.

### Task 4: Editor rehydration and completion UX

**Files:**
- Modify: `src/windows/OverlayWindow.tsx`
- Modify: `src/windows/EditorWindow.tsx`
- Modify: `src/windows/ToastWindow.tsx`
- Modify: `src/windows/LibraryWindow.tsx`
- Modify: `src/i18n/en.json`
- Modify: `src/i18n/zh-Hans.json`
- Modify: `src/i18n/ja.json`

**Step 1: Persist capture-time marks**

Atomically export the flattened PNG and its document from the same mark snapshot. Stage the bounded document before sending the existing raw PNG confirmation. Do not change the one-action clipboard-first toolbar.

**Step 2: Rehydrate editor projects**

Load and validate the document first. Use `kiri://annotation-source/<uuid>` only for a valid project; otherwise use the flattened asset as a legacy source. Scale marks into the current canvas and fall back to the flattened image with a visible non-destructive warning if project data is corrupt.

**Step 3: Clarify open actions**

Image completion thumbnails use an “Edit screenshot” accessible label. Image context-menu actions say “Edit”; video/GIF actions remain “Open”. Keep translations identical in key set.

**Step 4: Build**

Run: `pnpm build`

Expected: PASS with synchronized dictionaries.

### Task 5: Durable product documentation

**Files:**
- Create: `docs/adr/0016-editable-screenshot-projects.md`
- Modify: `docs/README.md`
- Modify: `docs/architecture.md`
- Modify: `README.md`
- Modify: `README_ZH.md`
- Modify: `README_EN.md`
- Modify only if already synchronized: `README_JA.md`

**Step 1: Record the interaction and persistence decision**

Document flattened-output compatibility, sidecar/source ownership, legacy behavior, Trash cleanup, and the fact that capture completion remains clipboard-first.

**Step 2: Update current product copy**

State that newly created annotations remain editable when reopening screenshots. Do not imply that historical flattened marks can be reconstructed.

**Step 3: Verify links and wording**

Run repository hygiene checks and search for stale “viewer for every image” descriptions.

### Task 6: Cleanup, native acceptance, Web docs, and delivery

**Files:**
- Modify only evidenced dead code/comments in task-touched Kiri files.
- Modify the actual `kiri-web` repository after inspecting its instructions and current status.
- Remove this completed plan from the working tree after implementation; Git history retains it.

**Step 1: Cleanup pass**

Run TypeScript build and Rust Clippy, search all new APIs and labels for call sites, remove unreachable branches/unused imports/stale comments, and keep compatibility fallbacks that still protect user data.

**Step 2: Run canonical Kiri checks**

Run:

```bash
pnpm test:release-tools
pnpm build
cargo test --locked --manifest-path src-tauri/Cargo.toml --all-targets
cargo check --locked --manifest-path src-tauri/Cargo.toml --all-targets
cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
git diff --check
```

Expected: all pass, Clippy reports zero warnings.

**Step 3: Package and test the fixed-path app**

Run `./scripts/install-app.sh`, open `/Applications/Kiri.app`, and verify: unannotated capture remains clipboard-first; annotated capture opens from its completion thumbnail; text/shape/mosaic marks can be modified after reopening; legacy screenshots still edit; copy/save updates the flat preview; Trash restore retains the project; permanent deletion removes it. Use only isolated agent-created captures and move them to recoverable Kiri Trash after QA.

**Step 4: Update and verify `kiri-web`**

Read its repository instructions, preserve dirty work, update only current product copy/visual evidence, run its canonical `vp` checks and ego-lite browser acceptance, and do not deploy unless a plain push is itself the documented deployment trigger and the user has authorized that effect.

**Step 5: Review and deliver both repositories**

Inspect exact diffs and outgoing commits, commit task-owned files on each current default branch, push, compare both local HEADs with live remote SHAs, and report any preserved dirty paths and native evidence boundaries.
