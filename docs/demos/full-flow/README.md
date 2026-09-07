# Complete interface recording

This documentation-only harness records the unmodified production frontend in one continuous multiwindow session. It does not ship with Kiri or change native recording/capture protection. The local sample and OCR text are original fixtures; screenshots are exported by the actual annotation canvas, and the sample video is encoded from browser frames captured during the same session.

Build the frontend at the source commit in the parent provenance file. Install Python 3.12, Playwright 1.55.0, Pillow 11.3.0, Chrome, ffmpeg and Noto Sans CJK. Set `KIRI_DEMO_DIST` to the built dist directory and `KIRI_DEMO_OUT` to a fresh local output directory. Also set `KIRI_SOURCE_COMMIT` and `KIRI_SOURCE_TREE` to the exact commit and tree used to build that frontend. In GitHub Actions, keep the supplied `GITHUB_RUN_ID` and `GITHUB_SHA`; for a local run, set `GITHUB_RUN_ID=0` (local, not a CI run) and `GITHUB_SHA` to the recorder's commit. `CHROME_BIN` optionally overrides the Chrome executable path.

Run `python docs/demos/full-flow/record.py` from the repository root, then set `FULL_FLOW_REVIEW` to the output directory and run `python docs/demos/full-flow/package.py`. Source metadata describes the frontend that was actually built; do not substitute an unrelated current branch SHA.

`--fast` checks the interactions but is not a publishable recording. The packager rejects incomplete flows, bad dimensions, missing countdown or pause/playback checks, renderer errors and mismatched timing. All network requests except local fixture resources are blocked. Never use personal files or credentials in the sample.

The published main MP4 includes the countdown and controls directly in its captured frames, not as separately rendered material pasted into an older video.

Publish the reviewed MP4 as a Release asset, record its URL in `provenance.json`, and point the root README video links to it. Keep the preview and poster in Git; do not add the generated MP4 to the repository.
