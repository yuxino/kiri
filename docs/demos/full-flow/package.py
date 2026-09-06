"""Encode a complete single-take recording; never modify app or capture protection."""
import hashlib,json,os,re,shutil,subprocess
from pathlib import Path
from PIL import Image
ROOT=Path(os.environ.get('FULL_FLOW_REVIEW','full-flow-review')).resolve()
OUT=Path(os.environ.get('FULL_FLOW_DELIVERY','full-flow-delivery')).resolve()
OUT.mkdir(parents=True,exist_ok=True)
r=json.loads((ROOT/'report.json').read_text(encoding='utf-8'))
assert r['success'] and not r['errors'] and not r['http_errors']
assert all(c['c']!='log_frontend_error' for c in r['calls'])
assert r['dimensions']==[1920,1080] and r['single_take'] and r['scene_cuts']==0
assert r['checks']['screenshot_saved'] and r['checks']['countdown_cancelled']
assert r['checks']['countdown_numerals']==[3,2,1] and r['checks']['pause_shown']
assert r['checks']['video_playback_ready'] and sorted(r['checks']['saved_assets'])==['image','video']
assert r['checks']['real_browser_subject_frames']>=4
frames=r['frames'];assert len(frames)>100
assert all(re.fullmatch(r'\d{6}\.jpg',f['file']) for f in frames)
assert all(b['t']>a['t'] for a,b in zip(frames,frames[1:]))
# Preserve the original countdown timing inside the same uninterrupted take.
# All other operations play 10x; no pasted overlays, cuts or inserted stills.
periods=[];current=None
for c in r['calls']:
 t=(c['at']-r['performance_start'])/1000
 if c['c']=='recording_countdown_ready':current=max(0,t-.2)
 if c['c'] in ['cancel_recording_flow','begin_recording'] and current is not None:
  periods.append([current,t+.1]);current=None
assert len(periods)==2
assert 2.9<periods[-1][1]-periods[-1][0]<3.7

def elapsed(a,b):return (b-a)/10+sum(max(0,min(b,y)-max(a,x))*.9 for x,y in periods)
def time_at(t):return elapsed(frames[0]['t'],t)
def run(args):subprocess.run(['ffmpeg','-v','error','-y',*args],check=True)
def sha(path):return hashlib.sha256(path.read_bytes()).hexdigest()
def info(path):return json.loads(subprocess.check_output(['ffprobe','-v','error','-show_format','-show_streams','-of','json',str(path)],text=True))
manifest=ROOT/'timeline.ffconcat';lines=['ffconcat version 1.0']
for i,f in enumerate(frames):
 end=frames[i+1]['t'] if i+1<len(frames) else r['raw_seconds'];dt=elapsed(f['t'],end);assert dt>0
 path=ROOT/'frames'/f['file'];assert path.is_file()
 lines += ["file '"+str(path).replace("'","'\\''")+"'",'option framerate 1000',f'duration {dt:.9f}']
lines.append("file '"+str(ROOT/'frames'/frames[-1]['file'])+"'")
manifest.write_text('\n'.join(lines)+'\n',encoding='utf-8')
run(['-f','concat','-safe','0','-i',str(manifest),'-an','-vf','fps=30,setsar=1','-c:v','libx264','-preset','medium','-crf','17','-pix_fmt','yuv420p','-movflags','+faststart','-threads','2',str(OUT/'demo.mp4')])
actual=info(OUT/'demo.mp4');stream=next(x for x in actual['streams'] if x['codec_type']=='video')
assert (stream['width'],stream['height'],stream['codec_name'])==(1920,1080,'h264')
duration=float(actual['format']['duration']);assert abs(duration-time_at(r['raw_seconds']))<.2
assert 18<duration<60,'The real walkthrough must be recorded, not fast validation mode'
poster_time=time_at((periods[-1][0]+periods[-1][1])/2)
run(['-ss',str(poster_time),'-i',str(OUT/'demo.mp4'),'-frames:v','1',str(OUT/'poster.png')])
run(['-i',str(OUT/'demo.mp4'),'-filter_complex','fps=15,scale=1440:810:flags=lanczos,split[a][b];[a]palettegen=max_colors=128:stats_mode=diff[p];[b][p]paletteuse=dither=bayer:bayer_scale=4:diff_mode=rectangle','-loop','0',str(OUT/'preview.gif')])
for name in ['demo.mp4','preview.gif']:run(['-i',str(OUT/name),'-f','null','-'])
assert abs(float(info(OUT/'preview.gif')['format']['duration'])-duration)<.15
assert all((OUT/n).stat().st_size<20_000_000 for n in ['demo.mp4','preview.gif','poster.png'])
scenes=[{'name':s['name'],'at':round(time_at(s['t']),3)} for s in r['scenes']]
p={'project':'kiri','source_commit':r['source_commit'],'source_tree':r['source_tree'],'source_build_run':r['artifact_run'],'source_build_artifact':r['artifact_id'],'capture_commit':os.environ.get('GITHUB_SHA'),'capture_run':os.environ.get('GITHUB_RUN_ID'),'recording_tool_sha256':sha(Path(__file__).with_name('record.py')),'native':False,'single_take':True,'scene_cuts':0,'recording_resolution':[1920,1080],'duration':duration,'sha256':sha(OUT/'demo.mp4'),'action_speed':10,'countdown_speed':1,'countdown_periods':periods,'added_holds':0,'scenes':scenes,'checks':r['checks'],'poster_time':round(poster_time,3),'previous_media_sha256':hashlib.sha256(Path('docs/demos/demo.mp4').read_bytes()).hexdigest(),'disclosure':'One uninterrupted recording of the unchanged production frontend, coordinated in an isolated multiwindow documentation harness. The same main recording visibly includes actual compact black CountdownWindow (112px, no separate cancel row) and ControlPanelWindow components, cancellation, 3-2-1, pause/resume/stop, saving, library and playback. Native IPC is substituted; this is not native end-to-end validation. The OCR text is an original sample fixture, not a fresh OCR engine result. Screenshot annotation export uses actual canvas bytes. The local sample video is encoded from frames captured during this browser session with paused frames omitted. No provider requests, user files, credentials or runtime capture-protection changes. Operations play 10x; countdowns retain their original timing without any cuts or inserted frames.'}
p['media']={n:{'bytes':(OUT/n).stat().st_size,'sha256':sha(OUT/n)} for n in ['demo.mp4','preview.gif','poster.png']}
(OUT/'provenance.json').write_text(json.dumps(p,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
(OUT/'README.md').write_text('''# Kiri demo

Screenshots, annotations, OCR interface, black recording countdown and cancellation, pause/resume, stop/save, library and playback in one continuous demonstration.

Recorded from the actual production frontend in an isolated multiwindow harness. Native APIs and the OCR response use original sample fixtures; the saved screenshot comes from the annotation canvas and the sample video is encoded from this session's browser frames. This is an interface walkthrough, not native end-to-end acceptance. App code and capture exclusion are unchanged.

`demo.mp4`, `preview.gif` and `poster.png` belong to the same recording. Detailed timing and source records are in `provenance.json`.
''',encoding='utf-8')
# Keep the exact recorder used so the checked-in documentation can reproduce it.
(OUT/'capture').mkdir(exist_ok=True);shutil.copyfile(Path(__file__).with_name('record.py'),OUT/'capture/record.py')
(OUT/'review-frames').mkdir(exist_ok=True)
for name in ['countdown-3','countdown-2','countdown-1','proof-10','proof-12','proof-13','proof-14']:
 shutil.copyfile(ROOT/(name+'.png'),OUT/'review-frames'/(name+'.png'))
print(json.dumps({'duration':duration,'scenes':scenes,'media':p['media']},ensure_ascii=False,indent=2))
