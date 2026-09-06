"""Copy verified documentation media only. No provider calls or recording is performed."""
from pathlib import Path
import hashlib,json,os,shutil,subprocess

PROJECTS={
 'kiri':(34011579006,9982660592,'b8d0fb08852940c475a61f0a89bd45532c94d7482be48430d8599512c8217149','0638d0202442d8a69c50230e630b8b5f64696f51a98fee3b55abc57e52391f64'),
 'tick':(34011579006,9982642963,'d06205b12f732f1da75b281bb0829f316d637d638f878f2077960f1d11d9976f','6e2d6a9ad99fb95fe415faf954b4ccb0f53335061bd22644bd8a3d8b8eed749f'),
 'mimi':(34012662273,9982960944,'39a87bb5294d99e5c2de5bf320d96ab252296ae7ee54cf4a08927b48064f357c','0fb2a2cd36692261f3b524ba794af3befb15a98e1eef194dae2f47c54c872261'),
 'satori':(34012662273,9982963504,'499da9afe14c34dd34611804cf1a3293c4ed71f47c0459cf41088fe3b278b3d1','6699d33831ac134329957717fdb916034dbedeb9a26685065367f1cfff9fc313'),
 'viva':(34012662273,9982960907,'b77e95077c25d44f7bc134d6976b3babd517d4ce04b8e43d94bfa182ec3aa36a','b1904c004f7d48d15b68d3935767e35c7687eaccede78ce0b8286dc377465d9a'),
}
repo=os.environ['GITHUB_REPOSITORY'];project=repo.split('/')[-1]
if project not in PROJECTS or repo!='yuxino/'+project:raise SystemExit('Unexpected repository')
if subprocess.check_output(['git','branch','--show-current'],text=True).strip()!='docs/hd-media-v4':raise SystemExit('Unexpected branch')
run,artifact,video_hash,poster_hash=PROJECTS[project]
source=Path(os.environ['HD_INPUT'])/(project if project in ['kiri','tick'] else '')
target=Path('docs/demos')
def sha(path):return hashlib.sha256(path.read_bytes()).hexdigest()
def info(path):
 return json.loads(subprocess.check_output(['ffprobe','-v','error','-show_streams','-show_format','-of','json',str(path)],text=True))
def verify_video(path,dimensions):
 data=info(path);v=next(x for x in data['streams'] if x['codec_type']=='video')
 if [v['width'],v['height']]!=dimensions:raise SystemExit('Unexpected media dimensions')
 subprocess.run(['ffmpeg','-hide_banner','-loglevel','error','-i',str(path),'-f','null','-'],check=True)
 return float(data['format']['duration'])
if sha(source/'demo.mp4')!=video_hash or sha(source/'poster.png')!=poster_hash:raise SystemExit('Reviewed media checksum differs')
r=json.loads((source/'report.json').read_text(encoding='utf-8'))
if not r.get('success') or not r.get('single_take') or r.get('speed_factor')!=10 or r.get('scene_cuts')!=0:raise SystemExit('Recording verification did not pass')
if r.get('javascript_errors'):raise SystemExit('Recording contains JavaScript errors')
seconds=verify_video(source/'demo.mp4',[1920,1080])
old=json.loads((target/'provenance.json').read_text())
if sha(target/'demo.mp4')!=old['sha256']:raise SystemExit('Existing media differs from its provenance')
shutil.copyfile(source/'demo.mp4',target/'demo.mp4')
shutil.copyfile(source/'poster.png',target/'poster.png')
filters='fps=12,scale=1440:-1:flags=lanczos,split[a][b];[a]palettegen=max_colors=192[p];[b][p]paletteuse=dither=bayer:bayer_scale=5:diff_mode=rectangle'
subprocess.run(['ffmpeg','-hide_banner','-loglevel','error','-y','-i',str(target/'demo.mp4'),'-filter_complex',filters,'-loop','0',str(target/'preview.gif')],check=True)
gif_seconds=verify_video(target/'preview.gif',[1440,810])
if abs(gif_seconds-seconds)>.3:raise SystemExit('Preview timing differs')
if any((target/n).stat().st_size>10_000_000 for n in ['demo.mp4','preview.gif','poster.png']):raise SystemExit('Media size budget exceeded')
r.pop('task_id',None)
r.update(project=project,capture_run=run,capture_artifact=artifact,duration=seconds,sha256=video_hash,previous_media_sha256=old['sha256'],preview_dimensions=[1440,810],preview_fps=12)
disclosure={
 'kiri':'Installed Windows app, actual screenshot/OCR/recording/library workflow. This recording predates the subsequent countdown redesign. Protected countdown and control windows are excluded from screen capture.',
 'tick':'Installed Windows app, manual Run now through the scheduler, reminder dialog created by the original example script, and real completion logs. No calendar-based trigger was awaited.',
 'viva':'Actual unmodified frontend with original in-memory notes. No native filesystem acceptance is claimed.',
 'mimi':'Actual frontend with offline replay of previously verified translation response text. This recording makes no new provider calls and does not verify native audio capture.',
 'satori':'Actual frontend with an original sample PDF and offline replay of previously verified answer text. No new provider calls or native end-to-end acceptance is claimed.',
}[project]
r['disclosure']=disclosure
r['media']={n:{'bytes':(target/n).stat().st_size,'sha256':sha(target/n)} for n in ['demo.mp4','preview.gif','poster.png']}
(target/'provenance.json').write_text(json.dumps(r,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
(target/'README.md').write_text('# '+project+' demo\n\n'+disclosure+'\n\nContinuous 1920×1080 recording, retimed uniformly at 10×. No scene joins or inserted holds. The animated preview is 1440×810. Source commits, checks and media hashes are recorded in `provenance.json`.\n',encoding='utf-8')
print(json.dumps({'project':project,'duration':seconds,'media':r['media']},indent=2))
