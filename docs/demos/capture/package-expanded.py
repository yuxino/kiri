"""Package successful expanded recordings; never publish failed or unlabelled scenes."""
from pathlib import Path
import hashlib
import json
import shutil
import subprocess
from PIL import Image

ROOT=Path(__file__).resolve().parent
OUT=ROOT/'expanded-deliverables'
PROJECTS=['kiri','mimi','satori','viva','tick','wnacg']
for repo in PROJECTS:
    source=ROOT/'expanded'/repo
    state=json.loads((source/'provenance.json').read_text())
    assert state['success'] and not state['javascript_errors'], repo
    assert state['speed_factor']==10 and len(state['scenes'])>=9, repo
    target=OUT/repo/'docs/demos';target.mkdir(parents=True,exist_ok=True)
    video=source/'demo.mp4'
    assert hashlib.sha256(video.read_bytes()).hexdigest()==state['sha256']
    info=json.loads(subprocess.check_output(['ffprobe','-v','error','-show_streams','-show_format','-of','json',str(video)],text=True))
    vs=info['streams'][0]
    assert vs['codec_name']=='h264' and vs['pix_fmt']=='yuv420p' and vs['width']==1280 and vs['height']==900
    assert 5<float(info['format']['duration'])<35 and len(info['streams'])==1
    subprocess.run(['ffmpeg','-v','error','-i',str(video),'-f','null','-'],check=True)
    shutil.copyfile(video,target/'demo.mp4');shutil.copyfile(source/'poster.png',target/'poster.png')
    filters='fps=12,scale=800:-1:flags=lanczos,split[a][b];[b]palettegen=max_colors=128[p];[a][p]paletteuse=dither=bayer:bayer_scale=4:diff_mode=rectangle'
    subprocess.run(['ffmpeg','-hide_banner','-loglevel','error','-y','-i',str(video),'-filter_complex',filters,'-threads','2','-loop','0',str(target/'preview.gif')],check=True)
    with Image.open(target/'preview.gif') as gif:
        assert gif.n_frames>40 and gif.info.get('loop')==0
        total=0
        for i in range(gif.n_frames):gif.seek(i);gif.load();total+=gif.info['duration']
        assert abs(total/1000-state['duration'])<.3
    assert all((target/name).stat().st_size<7_000_000 for name in ['demo.mp4','preview.gif','poster.png'])
    public={k:state[k] for k in ['project','source_commit','capture','limitations','browser','speed_factor','result_hold_seconds','duration','source_duration','sha256','scenes','actions']}
    public['disclosure']='New, expanded frontend interaction recording. Every action interval is retimed at 10x; each result receives a 0.8-second hold. Native API boundaries use original sample data. No live AI request, system capture, task execution or native filesystem acceptance is claimed.'
    public['recording_tool_sha256']=hashlib.sha256((ROOT/'expanded.py').read_bytes()).hexdigest()
    public['media']={p.name:{'bytes':p.stat().st_size,'sha256':hashlib.sha256(p.read_bytes()).hexdigest()} for p in target.iterdir() if p.is_file()}
    (target/'provenance.json').write_text(json.dumps(public,ensure_ascii=False,indent=2)+'\n')
    descriptions='\n'.join(f'{i+1}. {s["en"]} / {s["zh"]}' for i,s in enumerate(state['scenes']))
    (target/'README.md').write_text(f'''# {repo} expanded interface demonstration

This replaces the earlier three-to-four-scene, 2x demo with **{len(state['scenes'])} recorded scenes** from the actual production frontend at `{state['source_commit']}`.

**Pacing:** every source action interval is played at **10x**, followed by a **0.8-second result hold**. The final clip lasts {state['duration']:.2f} seconds; it is not a uniformly accelerated full video. The GIF and MP4 share the same timing. The fast-forward and sample-data labels remain visible.

**Scope:** {state['limitations']}. The browser harness substitutes native API boundaries with original local examples; this is not native macOS/Windows end-to-end validation. No user credentials, personal files, live provider output or upstream comic content are included. Satori question composition is shown without submitting an AI request; no answer is fabricated.

## Scenes

{descriptions}

## Files

`preview.gif` is the inline README preview; `demo.mp4` is the complete silent H.264 video. `poster.png` is an actual recorded result frame. `provenance.json` records source, pacing, scene boundaries and media hashes.

The reproducible documentation-only recorder is `yuxino/kiri/docs/demos/capture/expanded.py`. It is not loaded by the applications. The shipped application code, versions, signing and update workflows remain unchanged.
''')
    print(repo,len(state['scenes']),state['duration'],flush=True)
# Preserve the exact recorder that created these files, without the one-time patcher.
capture=OUT/'kiri/docs/demos/capture'
capture.mkdir(parents=True,exist_ok=True)
for name in ['expanded.py','package-expanded.py']:
    shutil.copyfile(ROOT/name,capture/name)
manifest={str(p.relative_to(OUT)):{'bytes':p.stat().st_size,'sha256':hashlib.sha256(p.read_bytes()).hexdigest()} for p in sorted(OUT.rglob('*')) if p.is_file()}
(OUT/'manifest.json').write_text(json.dumps(manifest,sort_keys=True,indent=2)+'\n')
print('Manifest SHA256:',hashlib.sha256((OUT/'manifest.json').read_bytes()).hexdigest())
