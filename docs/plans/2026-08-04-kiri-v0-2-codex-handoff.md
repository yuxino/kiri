# Kiri v0.2 Codex Handoff Implementation Plan

> **For Codex:** Read `AGENTS.md` first. Use the `executing-plans` workflow only
> when the user asks to continue implementation; do not mutate, commit, or push
> the current dirty worktree merely because this handoff exists.

**Goal:** Let a new Codex conversation safely continue Kiri v0.2 without losing
the current behavior, user decisions, verification state, or uncommitted work.

**Architecture:** Kiri is a Swift 6 macOS 14 application. `KiriApp` owns AppKit
and SwiftUI interaction, while `KiriCore` contains testable policies, geometry,
models, and persistence. ScreenCaptureKit and AVFoundation provide native still
capture, recording, MP4 output, and GIF conversion without external binaries.

**Tech Stack:** Swift 6, Swift Package Manager, AppKit, SwiftUI,
ScreenCaptureKit, AVFoundation, CoreMedia, CoreVideo, Carbon, ImageIO.

---

## Repository snapshot

- Date: 2026-08-04 (Asia/Shanghai)
- Original feature branch: `codex/v0-2-region-recording`
- Localized v0.2 baseline commit: `5af1287`
- The user authorized merging this work after silent click-to-select-window was
  restored. Inspect `git status -sb` and `git log` for the final merged state;
  do not assume the worktree is dirty or clean from this historical snapshot.
- Installed QA app: `/Applications/Kiri.app`, signed with the stable local Kiri
  development identity during the current session.
- User library: one original video was preserved. Agent-created QA recordings
  were moved to Kiri Trash and must not be permanently deleted without consent.

## Completed v0.2 behavior

- English and Simplified Chinese localization with macOS language selection.
- First-level Screenshot/Record switch in the selection overlay.
- Recording options for countdown, system audio, microphone, pointer, and click
  feedback.
- Centered non-dimming 3-2-1 countdown with Escape cancellation.
- Retina-scale recording dimensions, best capture resolution, and a bounded
  high-quality bitrate policy.
- Kiri application windows excluded from recordings, preventing the floating
  control/pause UI from entering exported frames.
- Live violet click ripple that is also included in the recording.
- Pause/resume implemented as MP4 segments merged into one final asset.
- Clipboard-first capture completion and background recording save behavior.
- Direct library-card move to recoverable Trash.
- Window clicks silently select the frontmost eligible window, while pointer
  movement shows no hover-following boxes. Manual drag, move, and eight-handle
  resize remain.
- README updates describing recording quality, countdown, and click feedback.

## Verified state

The following checks passed after the silent window-click refinement:

```text
swift run kiri-core-tests
40 tests passed

swift build --product kiri -Xswiftc -warnings-as-errors
Build of product 'kiri' complete

./scripts/package-app.sh
Release build and stable signing succeeded

./scripts/render-ui-snapshots.sh <temporary-directory>
8 offscreen PNG fixtures created

git diff --check
No whitespace errors
```

The offscreen snapshot command had become stale after localization because its
manual source list omitted new files and its direct compiler call did not set
the macOS 14 deployment target. Both problems are fixed in the current worktree.
Rerun the full verification block in `AGENTS.md` after future implementation.

## Known constraints and risks

- Microphone recording is enabled only on macOS 15 or later; other v0.2 capture
  paths support macOS 14.
- Borderless capture overlays are difficult for accessibility-based UI
  automation. Automated clicks can accidentally create recordings. Re-query UI
  state after every action and prefer careful visual/manual acceptance for the
  overlay itself.
- Historical capture plans and ADR 0001 still mention hover window previews.
  ADR 0003 explicitly supersedes that visual behavior while preserving click
  selection.
- `README_JA.md` has not been updated/localized for the full v0.2 feature set.
- MP4 trimming, full-display recording, inline playback, and recording safety
  limits remain roadmap items, not completed features.
- Do not use ad-hoc signing for normal QA; it can make macOS forget capture and
  input-monitoring permissions.

### Task 1: Orient before changing code

**Files:**

- Read: `AGENTS.md`
- Read: `docs/adr/0003-manual-region-selection.md`
- Read: this file

**Step 1:** Run `git status -sb` and compare it with the snapshot above.

**Step 2:** Inspect the relevant existing diff before editing; do not reset it.

**Step 3:** Restate the user's newest requested outcome and identify the
smallest files and tests that cover it.

### Task 2: Establish a fresh verification baseline

**Files:**

- Test: `Tests/KiriCoreTests/`
- QA: `scripts/render-ui-snapshots.sh`

**Step 1:** Run `swift run kiri-core-tests`.

Expected: 40 tests pass before additional tests are introduced.

**Step 2:** Run `swift build --product kiri -Xswiftc -warnings-as-errors`.

Expected: the Kiri executable builds with no warnings.

**Step 3:** Render offscreen snapshots into a temporary directory.

Expected: eight PNG fixtures are created without reading the user's library.

### Task 3: Implement only the next user-requested change

**Files:** Determine from the request; prefer one focused controller and its
corresponding `KiriCore` policy/test when business logic is involved.

**Step 1:** Add or update the smallest deterministic test where practical.

**Step 2:** Run the focused test and confirm the intended failure.

**Step 3:** Implement the minimal change while preserving the product contract
in `AGENTS.md`.

**Step 4:** Run focused and full verification.

**Step 5:** For visible UI behavior, package `/Applications/Kiri.app` and perform
proportional visual acceptance without leaving QA media in the library.

### Task 4: Handoff Git actions to the user

**Files:** All files intentionally changed for the requested feature.

**Step 1:** Run `git diff --check`, `git status -sb`, and summarize the diff.

**Step 2:** Commit only if the user asks. Use a focused message that describes
the user-visible behavior.

**Step 3:** Push, merge, tag, package a release, or publish only when explicitly
requested.
