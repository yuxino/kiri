"""Import this repository's audited documentation assets on the dedicated work branch only."""
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess

repo=os.environ['GITHUB_REPOSITORY']
if repo not in {f'yuxino/{x}' for x in ['kiri','mimi','satori','viva','tick','wnacg']}:
    raise SystemExit('Unexpected repository')
slug=repo.split('/')[1]
if subprocess.check_output(['git','branch','--show-current'],text=True).strip()!='docs/project-demo-v1':
    raise SystemExit('Refusing to write outside the documentation branch')
if subprocess.check_output(['git','status','--porcelain'],text=True).strip():
    raise SystemExit('Refusing to overwrite an unclean worktree')
bundle=Path(os.environ['DEMO_BUNDLE'])
manifest_bytes=(bundle/'manifest.json').read_bytes()
if hashlib.sha256(manifest_bytes).hexdigest()!=os.environ['DEMO_MANIFEST_SHA256']:
    raise SystemExit('Bundle manifest checksum mismatch')
manifest=json.loads(manifest_bytes)
entries={k:v for k,v in manifest.items() if k.startswith(slug+'/')}
if not entries:raise SystemExit('No media for this project')
for source,meta in entries.items():
    relative=Path(source).relative_to(slug)
    if '..' in relative.parts or not str(relative).startswith('docs/demos/'):
        raise SystemExit('Unexpected asset path')
    data=(bundle/source).read_bytes()
    if len(data)!=meta['bytes'] or hashlib.sha256(data).hexdigest()!=meta['sha256']:
        raise SystemExit(f'Corrupt asset: {source}')
    if relative.exists():raise SystemExit(f'Existing documentation would be overwritten: {relative}')
for source in entries:
    target=Path(source).relative_to(slug)
    target.parent.mkdir(parents=True,exist_ok=True)
    shutil.copyfile(bundle/source,target)

files={
 'kiri':{'README.md':'zh','README_ZH.md':'zh','README_EN.md':'en','README_JA.md':'ja'},
 'mimi':{'README.md':'zh','README_ZH.md':'zh','README_EN.md':'en','README_JA.md':'ja'},
 'satori':{'README.md':'en','README_ZH.md':'zh','README_JA.md':'ja'},
 'viva':{'README.md':'en','README_ZH.md':'zh'},
 'tick':{'README.md':'zh'},
 'wnacg':{'README.md':'zh','README.en.md':'en'},
}[slug]
features={
 'kiri':('框选、箭头、文字标注，以及撤销与重做。','Rectangle, arrow and text annotations, with undo and redo.','矩形・矢印・文字の注釈と、取り消し・やり直し。'),
 'mimi':('字幕语言、字号、对齐与沉浸模式设置。','Subtitle language, size, alignment and immersive-mode settings.','字幕の言語・サイズ・配置・没入モードの設定。'),
 'satori':('PDF 翻页、缩放、双页阅读与目录。','PDF page navigation, zoom, spreads and the outline.','PDF のページ送り・拡大・見開き表示・目次。'),
 'viva':('打开示例笔记、编辑 Markdown，以及 Source / Split / Live 切换。','Open a sample note, edit Markdown, and switch between Source, Split and Live.','サンプルノートを開き、Markdown を編集して Source・Split・Live を切り替えます。'),
 'tick':('手动配置任务、循环间隔与 Node.js 脚本。','Manual task configuration, recurring intervals and a Node.js script.','タスク・繰り返し間隔・Node.js スクリプトの設定。'),
 'wnacg':('原创示例页的连续、单页和双页阅读。','Continuous, single-page and spread layouts with original sample pages.','オリジナルのサンプルページで、連続・単ページ・見開き表示を切り替えます。'),
}[slug]
limits={
 'kiri':('不包含系统截图、OCR 或导出验收。','No native capture, OCR or export validation is shown.','OS の画面撮影・OCR・書き出しの検証ではありません。'),
 'mimi':('使用内置浏览器预览，不包含真实音频识别或翻译结果。','Uses the built-in browser preview; no real audio transcription or translation is shown.','内蔵のブラウザープレビューを使用し、実際の音声認識や翻訳結果は含みません。'),
 'satori':('使用原创样例 PDF，不调用 AI。','Uses an original sample PDF; no AI service is called.','オリジナルのサンプル PDF を使用し、AI サービスは呼び出しません。'),
 'viva':('文件由演示环境提供，不读写本机文件。','Files are supplied in memory by the demo environment; no native files are read or written.','ファイルはデモ用メモリ内データで、ローカルファイルの読み書きは行いません。'),
 'tick':('只演示配置，不保存或运行系统任务。','Configuration only: no system task is saved or executed.','設定画面のみのデモで、システムタスクの保存や実行は行いません。'),
 'wnacg':('不访问上游站点，不包含第三方漫画、OCR 或翻译结果。','No upstream site, third-party comics, OCR or translation results are used.','配信元サイトへのアクセス、第三者の漫画、OCR・翻訳結果は含みません。'),
}[slug]
labels={'zh':('演示','完整视频（MP4）','演示说明','真实前端录制，使用示例数据。'),'en':('Demo','Full video (MP4)','About this demo','Recorded from the actual frontend with sample data. '),'ja':('デモ','動画（MP4）','デモについて','実際のフロントエンドをサンプルデータで操作した録画です。')}
for filename,language in files.items():
    path=Path(filename);text=path.read_text(encoding='utf-8')
    if '<!-- project-demo-v1 -->' in text:raise SystemExit('Demo entry already exists')
    if slug=='viva':
        text=re.sub(r'<p align="center">\s*<img src="docs/images/viva-live-editor\.png"[^>]*>\s*</p>\s*','',text,count=1)
    elif slug=='tick':
        text=text.replace('![Tick 主界面](docs/images/tick-overview.png)\n\n','',1)
    index={'zh':0,'en':1,'ja':2}[language]
    heading,full,about,disclosure=labels[language]
    block=f'''<!-- project-demo-v1 -->
## {heading}

[![{slug} — {heading}](docs/demos/preview.gif)](docs/demos/demo.mp4)

[{full}](docs/demos/demo.mp4) · [{about}](docs/demos/README.md)

{features[index]} {disclosure}{limits[index]}
<!-- /project-demo-v1 -->

'''
    match=re.search(r'^## ',text,flags=re.M)
    if not match:raise SystemExit(f'No section anchor in {filename}')
    text=text[:match.start()]+block+text[match.start():]
    path.write_text(text,encoding='utf-8')
if slug in ['kiri','mimi'] and Path('README.md').read_bytes()!=Path('README_ZH.md').read_bytes():
    raise SystemExit('Chinese README aliases drifted')
if slug=='satori':
    status=Path('docs/status.md')
    status.write_text(status.read_text()+'''\n\n### 2026-09-06 · README 界面演示\n\n新增真实前端操作录屏、GIF 预览和来源记录，使用原创三页 PDF 展示翻页、缩放、双页与目录。录制环境替代了原生 API 边界，不调用 AI，也不代表原生系统端到端验收。应用代码、模型配置与发布版本均未改变。\n''')

subprocess.run(['git','add','--','docs/demos',*files,*(['docs/status.md'] if slug=='satori' else [])],check=True)
changed=subprocess.check_output(['git','diff','--cached','--name-only'],text=True).splitlines()
if not changed or any(not (p in files or p=='docs/status.md' and slug=='satori' or p.startswith('docs/demos/')) for p in changed):
    raise SystemExit('Unexpected staged change outside documentation')
subprocess.run(['git','diff','--cached','--check'],check=True)
subprocess.run(['git','-c','user.name=github-actions[bot]','-c','user.email=41898282+github-actions[bot]@users.noreply.github.com','commit','-m','docs: add recorded interface demo and animated README preview'],check=True)
subprocess.run(['git','push','origin','HEAD:refs/heads/docs/project-demo-v1'],check=True)
print(subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip())
