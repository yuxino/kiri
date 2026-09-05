"""One-time media import. Only the dedicated branch and explicit documentation paths."""
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess

repo=os.environ['GITHUB_REPOSITORY'];slug=repo.split('/')[-1]
projects=['kiri','mimi','satori','viva','tick','wnacg']
assert repo in [f'yuxino/{x}' for x in projects]
assert subprocess.check_output(['git','branch','--show-current'],text=True).strip()=='docs/demo-expanded-10x'
assert not subprocess.check_output(['git','status','--porcelain'],text=True).strip()
bundle=Path(os.environ['DEMO_BUNDLE'])
raw=(bundle/'manifest.json').read_bytes()
assert hashlib.sha256(raw).hexdigest()==os.environ['DEMO_MANIFEST_SHA256']
entries={k:v for k,v in json.loads(raw).items() if k.startswith(slug+'/')}
assert entries
allowed_media={'docs/demos/'+x for x in ['README.md','demo.mp4','poster.png','preview.gif','provenance.json']}
allowed_code={'docs/demos/capture/expanded.py','docs/demos/capture/package-expanded.py'} if slug=='kiri' else set()
for source,meta in entries.items():
    path=Path(source).relative_to(slug)
    assert str(path) in allowed_media|allowed_code
    data=(bundle/source).read_bytes()
    assert len(data)==meta['bytes'] and hashlib.sha256(data).hexdigest()==meta['sha256']
old=json.loads(Path('docs/demos/provenance.json').read_text())
new=json.loads((bundle/slug/'docs/demos/provenance.json').read_text())
assert old['project']==new['project']==slug and old['source_commit']==new['source_commit']
assert new['speed_factor']==10 and len(new['scenes'])>=9
for source in entries:
    dest=Path(source).relative_to(slug);dest.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(bundle/source,dest)
files={
 'kiri':{'README.md':'zh','README_ZH.md':'zh','README_EN.md':'en','README_JA.md':'ja'},
 'mimi':{'README.md':'zh','README_ZH.md':'zh','README_EN.md':'en','README_JA.md':'ja'},
 'satori':{'README.md':'en','README_ZH.md':'zh','README_JA.md':'ja'},
 'viva':{'README.md':'en','README_ZH.md':'zh'},
 'tick':{'README.md':'zh'},
 'wnacg':{'README.md':'zh','README.en.md':'en'},
}[slug]
features={
 'kiri':('矩形、线宽、箭头、文字、画笔、像素／模糊马赛克、撤销重做与裁剪入口。','Rectangles, line width, arrows, text, pen, pixel/blur mosaic, undo/redo and crop controls.'),
 'mimi':('语言组合、翻译模式、字号、三种对齐、沉浸与锁定、服务配置和界面语言。','Language pairs, translation modes, size, alignment, immersive/lock controls, service configurations and UI language.'),
 'satori':('翻页、缩放、单双页、目录跳转、问题输入与问答回看界面。','Pages, zoom, single/spread layouts, chapter jumps, question composition and the history panel.'),
 'viva':('源码编辑、四种视图、大纲、多标签、文内查找、专注模式与浅深色主题。','Source editing, four views, outline, tabs, find, focus mode and light/dark appearance.'),
 'tick':('每天／每月／每年／循环计划、脚本编辑、文件路径、高级参数、示例变量与日程。','Daily/monthly/yearly/interval schedules, script editing, file paths, advanced options, example variables and the calendar.'),
 'wnacg':('阅读宽度、留白／紧凑、连续／单双页、翻页、缩放及恢复比例。','Reading width, spacing, continuous/single/spread layouts, page navigation, zoom and reset.'),
}[slug]
limits={
 'kiri':('不包含原生截图、OCR 或导出验收。','No native capture, OCR or export validation.'),
 'mimi':('使用内置浏览器预览，不包含真实音频识别或翻译结果。','Uses the built-in browser preview; no live transcription or translation.'),
 'satori':('使用原创样例 PDF；仅展示提问操作，不提交真实 AI 请求或编造回答。','Uses an original sample PDF; question composition only, with no live AI request or fabricated answer.'),
 'viva':('使用内存中的示例文件，不读写本机文件。','Uses in-memory sample documents, not native filesystem access.'),
 'tick':('只演示配置，不保存或执行系统任务。','Configuration only; no system task is saved or executed.'),
 'wnacg':('不访问上游站点，不包含第三方漫画、OCR 或翻译结果。','No upstream site, third-party comics, OCR or translation results.'),
}[slug]
for name,lang in files.items():
    path=Path(name);text=path.read_text()
    heading={'zh':'演示','en':'Demo','ja':'デモ'}[lang]
    full={'zh':'完整视频（MP4）','en':'Full video (MP4)','ja':'動画（MP4）'}[lang]
    about={'zh':'演示说明','en':'About this demo','ja':'デモについて'}[lang]
    if lang=='zh':copy=f'{features[0]} **10× 操作快放，结果停留 0.8 秒。** 真实前端录制，使用示例数据。{limits[0]}'
    elif lang=='ja':copy=f'実際のフロントエンドをサンプルデータで操作。**操作は 10 倍速、結果画面は 0.8 秒保持。** {len(new["scenes"])} シーン。ネイティブ機能や AI の実動作検証ではありません。'
    else:copy=f'{features[1]} **10x actions with 0.8-second result holds.** Actual frontend with sample data. {limits[1]}'
    block=f'<!-- project-demo-v1 -->\n## {heading}\n\n[![{slug} — {heading}](docs/demos/preview.gif)](docs/demos/demo.mp4)\n\n[{full}](docs/demos/demo.mp4) · [{about}](docs/demos/README.md)\n\n{copy}\n<!-- /project-demo-v1 -->'
    pattern=r'<!-- project-demo-v1 -->.*?<!-- /project-demo-v1 -->'
    assert len(re.findall(pattern,text,re.S))==1,name
    path.write_text(re.sub(pattern,lambda _:block,text,flags=re.S))
    for target in ['docs/demos/preview.gif','docs/demos/demo.mp4','docs/demos/README.md']:assert Path(target).is_file()
extra=[]
if slug=='satori':
    p=Path('docs/status.md');p.write_text(p.read_text()+'\n\n### 2026-09-06 · 扩展示范与 10× 节奏\n\n演示扩展到阅读、目录跳转、问题输入和回看界面，操作段 10×、结果短暂停留。未提交真实 AI 请求；未使用或保存用户密钥。仅更新文档与媒体，应用行为和发布不变。\n');extra.append(str(p))
if slug=='kiri':
    p=Path('docs/demos/capture/README.md');p.write_text(p.read_text()+'\n\n## Expanded 10x tour\n\nThe current README clips use `python -I expanded.py` followed by `python -I package-expanded.py`. Run `fixtures.py` first and provide the same pinned production builds described above. `expanded.py` records more UI scenes, accelerates each action interval by 10, and adds a 0.8-second result hold. The older `retime.py` documents the previous 2x edit and is not applied to these new clips. All original sample-data and no-native-acceptance boundaries still apply.\n');extra.append(str(p))
remove=['.github/workflows/import-expanded-demo.yml']
if slug=='kiri':remove+=['.github/workflows/expanded-demo.yml','docs/demos/capture/refine-expanded.py','docs/demos/capture/import-expanded.py']
for name in remove:
    path=Path(name)
    if path.exists():path.unlink()
subprocess.run(['git','add','-A','--',*allowed_media,*allowed_code,*files,*extra,*remove],check=True)
paths=subprocess.check_output(['git','diff','--cached','--name-only'],text=True).splitlines()
assert paths and set(paths)<=allowed_media|allowed_code|set(files)|set(extra)|set(remove)
subprocess.run(['git','diff','--cached','--check'],check=True)
subprocess.run(['git','-c','user.name=github-actions[bot]','-c','user.email=41898282+github-actions[bot]@users.noreply.github.com','commit','-m','docs: replace demos with richer 10x interaction tours'],check=True)
subprocess.run(['git','push','origin','HEAD:refs/heads/docs/demo-expanded-10x'],check=True)
print(subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip())
