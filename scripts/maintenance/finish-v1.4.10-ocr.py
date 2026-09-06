from pathlib import Path
import json,subprocess
r=Path.cwd()
def edit(path,old,new):
 p=r/path;s=p.read_text();assert s.count(old)==1,(path,s.count(old));p.write_text(s.replace(old,new))
path='src/windows/OverlayWindow.tsx'
edit(path,'''  // Auto-run OCR when the user switches to OCR mode with an existing valid
  // selection (reuse instead of re-drawing the region). Fires once per
  // transition because runOcr moves the phase to "ocr-result".
  useEffect(() => {
    if (mode === "ocr" && phase === "selecting" && selection && isValidSelection(selection, 3)) {
      void runOcr(selection);
    }
  }, [mode, phase, selection, runOcr]);

''','')
edit(path,'''      if (drag) {
        const moved = Math.hypot(p.x - drag.start.x, p.y - drag.start.y) >= 3;
        if (!moved && context && modeRef.current !== "ocr") {''','''      if (drag) {
        const moved = Math.hypot(p.x - drag.start.x, p.y - drag.start.y) >= 3;
        const committedSelection = normalized(drag.start, p);
        if (!moved && context && modeRef.current !== "ocr") {''')
edit(path,'''        } else if (moved && selectionRef.current && isValidSelection(selectionRef.current, 3)) {
          // OCR (and screenshot/record) recognize only after an actual drag
          // release — a plain click must not trigger recognition.
          afterSelection(selectionRef.current);''','''        } else if (moved && isValidSelection(committedSelection, 3)) {
          // Commit the release endpoint, not a partial/stale pointer-move render.
          // A plain click never starts OCR, and dragging cannot prepare a crop.
          setSelection(committedSelection);
          selectionRef.current = committedSelection;
          afterSelection(committedSelection);''')
edit(path,'''  // Spec §2.4 changeCaptureMode: switching the mode resets to the selection
  // phase, tears down the recording-options popover and the OCR panel, and
  // keeps or clears the region per mode (OCR always clears it). The mode
  // selector stays visible throughout (spec §1.2), so this can be invoked
  // at any point.''','''  // Switching modes clears transient UI while reusing a completed selection.
  // OCR starts only from this explicit action or a committed pointer release.''')
edit(path,'''        // Reuse an existing region when switching into OCR: run recognition
        // on it right away instead of forcing a fresh drag (the effect below
        // watches for a valid selection in OCR+selecting). Only clear when
        // there is no usable selection yet.
        if (selectionRef.current && isValidSelection(selectionRef.current, 3)) {
          setPhase("selecting");''','''        // Reuse the finished crop once, without a render effect watching live
        // selection changes during a new OCR drag.
        if (selectionRef.current && isValidSelection(selectionRef.current, 3)) {
          void runOcr(selectionRef.current);''')
edit(path,'    [completionLock, discardPreparedOcr],','    [completionLock, discardPreparedOcr, runOcr],')
edit(path,'''      <div
        style={{
          background: "rgba(255,255,255,0.97)",''','''      <div
        role="region"
        aria-label={t("Recognized Text")}
        tabIndex={0}
        style={{
          background: "rgba(255,255,255,0.97)",''')
p=r/'package.json';d=json.loads(p.read_text());assert d['version']=='1.4.10';d['scripts']['test:release-tools']+=' scripts/ocr-selection-commit.test.mjs';p.write_text(json.dumps(d,indent=2)+'\n')
edit('docs/demos/full-flow/record.py',"o=await capture('文字');await drag(194,104,1090,641);await o.get_by_text('识别结果',exact=True).wait_for();await mark('整页文字识别：标题、清单和段落',12)","""o=await capture('文字')
   await move(194,104);await page.mouse.down();await page.mouse.move(220,120,steps=5);await wait(1)
   assert await page.evaluate(\"state.calls.filter(x=>x.c==='prepare_ocr_request').length\")==0,'Premature OCR while pointer held'
   await page.mouse.move(1090,641,steps=36);await wait(1)
   assert await page.evaluate(\"state.calls.filter(x=>x.c==='prepare_ocr_request').length\")==0,'Premature OCR before release'
   await page.mouse.up();await o.get_by_text('识别结果',exact=True).wait_for()
   requests=await page.evaluate(\"state.calls.filter(x=>x.c==='prepare_ocr_request')\");assert len(requests)==1,requests
   actual=requests[0]['a']['selection'];assert actual=={'x':194,'y':104,'width':896,'height':537},actual
   report['checks']['ocr_request_count']=len(requests);report['checks']['ocr_selection']=actual
   await mark('整页文字识别：标题、清单和段落',8)
   result=o.get_by_role('region',name='识别结果',exact=True);await result.hover();await page.mouse.wheel(0,240);await mark('回看识别内容并复制',8)""")
edit('docs/demos/full-flow/record.py',"   report['checks']['ocr_selection']={'x':194,'y':104,'width':896,'height':537}\n",'')
edit('docs/demos/full-flow/package.py',"assert r['checks']['screenshot_saved'] and r['checks']['no_recording_cancel_demo'] and r['checks']['ocr_lines']>=12","""assert r['checks']['screenshot_saved'] and r['checks']['no_recording_cancel_demo'] and r['checks']['ocr_lines']>=12
assert r['checks']['ocr_request_count']==1
assert r['checks']['ocr_selection']=={'x':194,'y':104,'width':896,'height':537}""")
edit('docs/architecture.md','Local OCR is the default and runs through macOS Vision or Windows.Media.Ocr.\nThe normal local path does not use the network.','''Local OCR is the default and runs through macOS Vision or Windows.Media.Ocr.
The normal local path does not use the network. OCR crop preparation waits for
pointer release and uses that final endpoint, never an intermediate drag
frame. Explicitly switching a completed screenshot selection to OCR reuses
that crop once. Plain clicks and partial drags do not prepare or send a crop
(ADR 0028).''')
edit('docs/releases/v1.4.10.md','## 更新内容\n','## 更新内容\n\n- 修复 OCR 拖选中途提前识别的问题：松开鼠标后按完整选区识别一次，切换模式仍可复用已有选区。\n')
edit('.github/workflows/build.yml','python -m pip install playwright==1.55.0 imageio-ffmpeg==0.6.0','python -m pip install playwright==1.55.0 imageio-ffmpeg==0.6.0 Pillow==11.3.0')
edit('.github/workflows/build.yml','      - run: python scripts/qa/countdown-ui.py --record\n','''      - run: python scripts/qa/countdown-ui.py --record
      - name: Verify OCR waits for a committed selection
        run: python docs/demos/full-flow/test-ocr.py
        env:
          KIRI_DEMO_DIST: ${{ github.workspace }}/dist
          KIRI_DEMO_OUT: ${{ runner.temp }}/ocr-selection-review
''')
edit('.github/workflows/build.yml','            countdown-review/\n','            countdown-review/\n            ${{ runner.temp }}/ocr-selection-review/\n')
(r/'docs/demos/full-flow/.gitignore').write_text('__pycache__/\n*.pyc\n')
for name in ['docs/demos/full-flow/__pycache__/package.cpython-312.pyc','docs/demos/full-flow/__pycache__/record.cpython-312.pyc','docs/demos/capture/record.py']:
 p=r/name
 if p.exists():p.unlink()
p=r/'docs/adr/0028-ocr-committed-selection.md';assert not p.exists();p.write_text('''# ADR 0028: OCR starts from a committed selection

- Status: Accepted
- Date: 2026-09-06

## Problem

A render effect started OCR whenever a valid selection existed in OCR mode.
The first small pointer movement therefore prepared a tiny crop before the
user finished dragging. It could also race the pointer-release action and
issue a duplicate preparation request.

## Decision

Remove render-driven OCR. A new OCR region is committed on pointer release,
using the normalized start and final pointer position rather than a potentially
stale selection render. A plain click does not recognize. Explicitly switching
a completed screenshot region to OCR prepares that existing region once.

Keep selection movement/resizing, local OCR, remote-provider consent, request
cancellation and native capture boundaries unchanged. The recognized-text
surface remains scrollable and gains a labelled keyboard-focusable region.

## Verification

Regression tests exercise slow partial drags without a request, one full crop
on release, plain clicks, reverse-direction selection, and reuse of a completed
selection. The documentation capture asserts the actual IPC crop and request
count instead of trusting its planned mouse coordinates. Browser checks use
the production frontend with isolated documentation IPC, not a native OCR
engine or real provider request.
''')
subprocess.run(['node','scripts/release-version.mjs','v1.4.10'],check=True)
subprocess.run(['git','diff','--check'],check=True)
