"""Create reviewable, bounded documentation assets, never application release artifacts."""
from pathlib import Path
import hashlib
import json
import shutil
import subprocess

root=Path(__file__).resolve().parent
out=root/'deliverables'
projects=['kiri','mimi','satori','viva','tick','wnacg']
for repo in projects:
    src=root/'recordings'/repo
    state=json.loads((src/'provenance.json').read_text())
    if not state.get('success') or state.get('javascript_errors'):
        raise SystemExit(f'{repo}: capture did not pass; refusing to package')
    video=src/'demo.mp4'
    if hashlib.sha256(video.read_bytes()).hexdigest()!=state['sha256']:
        raise SystemExit(f'{repo}: video checksum mismatch')
    if not 8 <= state['duration'] <= 65:
        raise SystemExit(f'{repo}: unexpected duration')
    target=out/repo/'docs/demos'
    target.mkdir(parents=True,exist_ok=True)
    shutil.copyfile(video,target/'demo.mp4')
    shutil.copyfile(src/'poster.png',target/'poster.png')
    filters='fps=6,scale=800:-1:flags=lanczos,split[x][z];[z]palettegen=max_colors=96[p];[x][p]paletteuse=dither=bayer:bayer_scale=5:diff_mode=rectangle'
    subprocess.run(['ffmpeg','-hide_banner','-loglevel','error','-y','-i',str(video),'-filter_complex',filters,'-threads','2','-loop','0',str(target/'preview.gif')],check=True)
    if (target/'preview.gif').stat().st_size > 5_000_000:
        filters=filters.replace('fps=6,scale=800','fps=4,scale=720')
        subprocess.run(['ffmpeg','-hide_banner','-loglevel','error','-y','-i',str(video),'-filter_complex',filters,'-threads','2','-loop','0',str(target/'preview.gif')],check=True)
    if any((target/name).stat().st_size > 8_000_000 for name in ['demo.mp4','preview.gif','poster.png']):
        raise SystemExit(f'{repo}: media exceeded documentation size budget')
    provenance={key:state[key] for key in ['project','source_commit','capture','limitations','steps','browser','duration','sha256']}
    provenance['recording_tool_sha256']=hashlib.sha256((root/'capture.py').read_bytes()).hexdigest()
    provenance['fixture_sha256']={p.name:hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted((root/'fixtures').iterdir()) if p.is_file()}
    provenance['media']={p.name:{'bytes':p.stat().st_size,'sha256':hashlib.sha256(p.read_bytes()).hexdigest()} for p in sorted(target.iterdir()) if p.is_file()}
    provenance['disclosure']='Real frontend interaction recording with synthetic local data and mocked native boundaries. Not native end-to-end validation. No personal data, external content, provider requests, audio capture or task execution.'
    (target/'provenance.json').write_text(json.dumps(provenance,ensure_ascii=False,indent=2)+'\n')
    (target/'README.md').write_text(f'''# {repo} interface demonstration

The animated preview and MP4 show interactions with the actual production frontend at `{state['source_commit']}`. The recording uses original sample documents and documentation-only substitutes for native API boundaries, not a native macOS/Windows session.

**Scope:** {state['limitations']}.

The visible sample-data label remains throughout the recording. No user files, private content, credentials or live AI output are included. This is a product-interface walkthrough, not evidence of native capture, filesystem persistence, scheduling or cross-version update acceptance.

- `preview.gif`: inline README preview.
- `demo.mp4`: full H.264 recording, without audio.
- `poster.png`: still from the recording.
- `provenance.json`: source commit, actual steps, capture engine and media hashes.

The shared documentation-only recorder and original fixture generators live in `yuxino/kiri` under `docs/demos/capture/`. The tooling is not bundled with the applications.
''')
    print(repo,round(state['duration'],1),provenance['media'])

capture=out/'kiri/docs/demos/capture'
capture.mkdir(exist_ok=True)
for name in ['bridge.js','capture.py','fixtures.py','package.py']:
    shutil.copyfile(root/name,capture/name)
(capture/'README.md').write_text('''# Documentation-only recorder

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
''')
# Bundle-level manifest covers every file before another repository imports it.
manifest={str(p.relative_to(out)):{'bytes':p.stat().st_size,'sha256':hashlib.sha256(p.read_bytes()).hexdigest()} for p in sorted(out.rglob('*')) if p.is_file()}
(out/'manifest.json').write_text(json.dumps(manifest,sort_keys=True,indent=2)+'\n')
