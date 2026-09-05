# Documentation-only recorder

This records real built frontends with explicit sample data. It is never imported by application code. The native integration layer is replaced only inside disposable browser contexts, with all external requests blocked except exact local fixture routes. Mimi uses its existing browser-preview mode.

To reproduce a scene, build each project at the source commit recorded in its `docs/demos/provenance.json`, then place its `dist` output in `dist/demo-frontend-<project>/` next to these scripts and write that commit to `source-commit.txt`. Projects: kiri, mimi, satori, viva, tick, wnacg.

Use Python 3.12, Playwright 1.55.0, Pillow 11.3.0, ReportLab 4.4.3, ffmpeg, current stable Google Chrome, DejaVu Sans and Noto CJK fonts. Install Playwright's ffmpeg helper with `python -m playwright install ffmpeg`. Run from this directory:

```sh
python -I fixtures.py
python -I capture.py
python -I package.py
```

The recorder binds only localhost. It captures genuine clicks, typing, canvas drawing and reader navigation; the surrounding project title, bilingual step caption, pointer indicator and fixture disclosure are documentation overlays. It does not access a real desktop, native filesystem, task scheduler, microphone, provider account or upstream comic site. Do not remove those disclosures or present these recordings as native end-to-end acceptance.

Review every resulting poster and the full videos before updating media. Failed scenes are not packaged. The capture scripts do not push commits, publish releases or change project versions.
