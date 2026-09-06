"""One-time, bounded preparation of the requested demo/release candidate.
Only documentation capture, version metadata and the release note are edited.
The application's capture/cancel behavior, keys and release gates stay intact.
"""
from pathlib import Path
import json, subprocess
ROOT=Path.cwd()
def edit(path,old,new):
 p=ROOT/path;s=p.read_text(encoding='utf-8');assert s.count(old)==1,(path,old[:100],s.count(old));p.write_text(s.replace(old,new),encoding='utf-8')
p=ROOT/'docs/demos/full-flow/record.py';s=p.read_text(encoding='utf-8')
a=s.index("   c=page.frame_locator('#countdown');await c.get_by_role('button',name='取消倒计时').wait_for();await page.wait_for_timeout(250)")
b=s.index("   c=page.frame_locator('#countdown');await c.get_by_role('button',name='取消倒计时').wait_for();seen=[]",a)
s=s[:a]+s[b:]
old="o=await capture('文字');await drag(210,150,1015,292);await o.get_by_text('识别结果',exact=True).wait_for();await mark('文字识别结果')";assert s.count(old)==1
s=s.replace(old,"o=await capture('文字');await drag(194,104,1090,641);await o.get_by_text('识别结果',exact=True).wait_for();await mark('整页文字识别：标题、清单和段落',12)")
s=s.replace("await click(o.get_by_role('button',name='复制',exact=True));await wait(2)","await click(o.get_by_role('button',name='复制',exact=True));await wait(2)\n   copied=await page.evaluate('state.copiedText');report['checks']['ocr_lines']=len(copied.splitlines());assert report['checks']['ocr_lines']>=12\n   report['checks']['ocr_selection']={'x':194,'y':104,'width':896,'height':537}")
old="await page.evaluate('openLibrary()');await mark('截图和录屏都在素材库')";assert s.count(old)==1
s=s.replace(old,"await page.evaluate('openLibrary()');await page.frame_locator('#library').get_by_role('button',name='设置',exact=True).wait_for();await wait(1)\n   report['ending_start']=time.monotonic()-start;await mark('截图和录屏都在素材库',6)\n   report['checks']['no_recording_cancel_demo']=await page.evaluate(\"!state.calls.some(x=>x.c==='cancel_recording_flow')\");assert report['checks']['no_recording_cancel_demo']")
assert 'countdown_cancelled' not in s;p.write_text(s,encoding='utf-8')
ocr='把有用的细节，留在眼前。\\n圈出重点，复制文字，再用一段录屏说明过程。\\n今天的小目标\\n整理界面里的关键内容\\n用标注说明一个想法\\n录下完整的操作过程\\n一段值得留下的话\\n不需要把所有事情一次做完。\\n先完成眼前这一件，再开始下一件。\\n完成整理\\n慢慢来，也很好。\\n随手记下，随时回看。'
edit('docs/demos/full-flow/desktop.js',"return '把有用的细节，留在眼前。\\n圈出重点，复制文字，再用一段录屏说明过程。';",f"return '{ocr}';")
edit('docs/demos/full-flow/desktop.js',"if(c==='plugin:app|version')return '1.4.9';","if(c==='plugin:app|version')return '1.4.10';")
p=ROOT/'docs/demos/full-flow/package.py';s=p.read_text(encoding='utf-8')
s=s.replace("r['checks']['screenshot_saved'] and r['checks']['countdown_cancelled']","r['checks']['screenshot_saved'] and r['checks']['no_recording_cancel_demo'] and r['checks']['ocr_lines']>=12")
s=s.replace('assert len(periods)==2','assert len(periods)==1')
old="def elapsed(a,b):return (b-a)/10+sum(max(0,min(b,y)-max(a,x))*.9 for x,y in periods)";assert s.count(old)==1
s=s.replace(old,"ending=[r['ending_start'],r['raw_seconds']]\nassert 5.9 <= ending[1]-ending[0] < 10\nassert ending[0]>periods[-1][1]\nslow_periods=periods+[ending]\ndef elapsed(a,b):return (b-a)/10+sum(max(0,min(b,y)-max(a,x))*.9 for x,y in slow_periods)")
s=s.replace("'added_holds':0,'scenes':scenes","'added_holds':0,'ending_hold_seconds':round(ending[1]-ending[0],3),'scenes':scenes")
s=s.replace('ControlPanelWindow components, cancellation, 3-2-1, pause/resume/stop','ControlPanelWindow components, one 3-2-1, pause/resume/stop')
s=s.replace('The OCR text is an original sample fixture, not a fresh OCR engine result.','The OCR text is a 12-line original sample fixture covering the visible title, checklist and paragraphs, not a fresh OCR engine result.')
s=s.replace('Operations play 10x; countdowns retain their original timing without any cuts or inserted frames.','Operations play 10x; the countdown and six-second final-library view retain their captured timing without cuts or inserted stills. No recording-cancellation scene is performed.')
s=s.replace('black recording countdown and cancellation','compact black recording countdown')
old="for name in ['countdown-3','countdown-2','countdown-1','proof-10','proof-12','proof-13','proof-14']:\n shutil.copyfile(ROOT/(name+'.png'),OUT/'review-frames'/(name+'.png'))";assert s.count(old)==1
s=s.replace(old,"for path in sorted(ROOT.glob('*.png')):\n if path.name.startswith(('countdown-','proof-')):\n  shutil.copyfile(path,OUT/'review-frames'/path.name)")
p.write_text(s,encoding='utf-8')
for name in ['package.json','src-tauri/tauri.conf.json']:
 assert json.loads((ROOT/name).read_text())['version']=='1.4.9';edit(name,'"version": "1.4.9"','"version": "1.4.10"')
edit('src-tauri/Cargo.toml','version = "1.4.9"','version = "1.4.10"')
edit('src-tauri/Cargo.lock','name = "kiri"\nversion = "1.4.9"','name = "kiri"\nversion = "1.4.10"')
p=ROOT/'docs/releases/v1.4.10.md';assert not p.exists();p.parent.mkdir(exist_ok=True)
p.write_text('''# Kiri v1.4.10

## 更新内容

- 录屏倒计时改为紧凑黑色圆环，不再显示单独的取消按钮；点击圆环或按 Esc 仍可取消。
- 倒计时在界面显示后才开始，修复启动时序和录制控制条的状态同步。
- Windows 安装程序采用统一的浅色插画主题。
- 更新完整演示：移除取消录制片段，扩大 OCR 选区、展示更多文字，延长结尾停留。

## 安装与更新

使用已有 v1.4.9 的用户可在设置中手动检查并安装签名更新。更早版本需先手动升级一次。更新方式、签名公钥、素材库格式和取消录制功能保持不变。

macOS 正式包必须由维护者使用原有长期签名身份打包，不用临时签名替代。Windows 安装包具有更新器签名，但没有 Authenticode 签名。发布草稿须等相应平台产物及验证齐备后再转为正式版本。
''',encoding='utf-8')
subprocess.run(['node','scripts/release-version.mjs','v1.4.10'],check=True)
subprocess.run(['git','diff','--check'],check=True)
print('Prepared demo refinement and v1.4.10 source metadata. No tag or release published.')
