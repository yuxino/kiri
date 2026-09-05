"""Apply verified recorder-only corrections; the six application builds are untouched."""
from pathlib import Path
import ast
import subprocess
import sys
root = Path(__file__).resolve().parent
source = (root / 'record.py').read_text()
patches = {
 "frame.get_by_role('button',name='Source',exact=True)": "frame.get_by_label('Source',exact=True)",
 "frame.get_by_role('button',name='Split',exact=True)": "frame.get_by_label('Split',exact=True)",
 "frame.get_by_role('button',name='Live',exact=True)": "frame.get_by_label('Live',exact=True)",
 "frame.get_by_role('textbox',name='Markdown editor',exact=True)": "frame.get_by_role('textbox',name='Editing Weekend.md',exact=True)",
 "frame.get_by_role('button',name='取消',exact=True).last": "frame.get_by_role('button',name=re.compile(r'^取\\s*消$')).last",
 "await frame.get_by_placeholder('Type something…').press_sequentially('Start small.',delay=95)\n            await page.keyboard.press('Escape');await page.wait_for_timeout(1500);await shot('poster')": "await frame.get_by_placeholder('Type something…').press_sequentially('Start small.',delay=95)\n            await click(frame.get_by_title('Select (V)'),1500);await shot('poster')",
 "result=subprocess.run(['ffmpeg'": "result=await asyncio.to_thread(subprocess.run,['ffmpeg'",
}
for old,new in patches.items():
    if source.count(old) != 1:
        raise SystemExit(f'Recorder changed; review the correction first: {old}')
    source = source.replace(old,new)
ast.parse(source)
path = root / 'capture.py'
path.write_text(source,encoding='utf-8')
subprocess.run([sys.executable,'-I',str(path)],check=True)
