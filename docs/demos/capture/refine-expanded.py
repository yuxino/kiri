"""One-time capture corrections, applied only to documentation tooling."""
from pathlib import Path
import ast
root=Path(__file__).resolve().parent
p=root/'expanded.py';text=p.read_text()
patches={
"close=f.get_by_label('收起目录',exact=True)\n            if await close.is_visible():await click(close)\n            ":"", # Choosing a chapter already closes this drawer.
"f.get_by_role('tab',name='Weekend.md',exact=True)":"f.get_by_role('tab',name=re.compile('^Weekend\\.md'))",
"f.get_by_role('button',name='添加变量',exact=True)":"f.get_by_role('button',name=re.compile('添加变量'))",
"await click(f.get_by_label('放大阅读页面',exact=True));await click(f.get_by_label('放大阅读页面',exact=True))":"await click(f.get_by_label('阅读设置',exact=True).first);await click(f.get_by_role('button',name='连续',exact=True));await page.keyboard.press('Escape');await click(f.get_by_label('放大阅读页面',exact=True));await click(f.get_by_label('放大阅读页面',exact=True))",
"await click(f.get_by_label('Line',exact=True));await f.get_by_label('Line',exact=True).press('ArrowRight')":"await click(f.get_by_label('Line',exact=True));await f.get_by_label('Line',exact=True).press('Home');await f.get_by_label('Line',exact=True).press('ArrowRight')",
"调整线宽，不必重新画":"调整接下来绘制的线宽",
"await drag(.69,.59,.55,.43)":"await drag(.81,.52,.68,.40)",
"await drag(.135,.875,.50,.875)":"await drag(.135,.421,.43,.421)",
"await drag(.57,.875,.83,.875)":"await drag(.57,.725,.83,.725)",
}
for old,new in patches.items():
    if text.count(old)!=1:raise SystemExit('Source changed; review patch: '+old)
    text=text.replace(old,new)
ast.parse(text);p.write_text(text)
p=root/'package-expanded.py';text=p.read_text()
anchor="manifest={str(p.relative_to(OUT))"
assert text.count(anchor)==1
text=text.replace(anchor,"""# Preserve the exact recorder that created these files, without the one-time patcher.
capture=OUT/'kiri/docs/demos/capture'
capture.mkdir(parents=True,exist_ok=True)
for name in ['expanded.py','package-expanded.py']:
    shutil.copyfile(ROOT/name,capture/name)
manifest={str(p.relative_to(OUT))""")
ast.parse(text);p.write_text(text)
