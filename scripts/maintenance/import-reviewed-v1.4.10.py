"""Import one checksum-pinned, already reviewed CI delivery; never edits workflows.
The capture finished and passed; its token could not push a workflow change.
That workflow is now updated separately through the authorized connector.
"""
from pathlib import Path
import hashlib, json, os, shutil, subprocess, tarfile, zipfile
ROOT=Path.cwd();TMP=Path(os.environ['RUNNER_TEMP'])/'review-import';TMP.mkdir(exist_ok=True)
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
archive=TMP/'review.zip'
with archive.open('wb') as out:subprocess.run(['gh','api','repos/yuxino/kiri/actions/artifacts/9991358377/zip'],stdout=out,check=True)
assert sha(archive)=='fbc06e32c48d613d47a96e6e2a245e01a8ad4c648759c2c53e3b92d7c62aa924'
review=TMP/'review';review.mkdir()
with zipfile.ZipFile(archive) as z:
 for n in z.namelist():assert not Path(n).is_absolute() and '..' not in Path(n).parts
 z.extractall(review)
source_tar=review/'source.tar';assert sha(source_tar)=='6e702897f62c5806096414ea60d0f24ccfb665841ab6f201764829771cc10575'
source=TMP/'source';source.mkdir()
with tarfile.open(source_tar) as t:
 for m in t.getmembers():assert (m.isfile() or m.isdir()) and not Path(m.name).is_absolute() and '..' not in Path(m.name).parts
 t.extractall(source,filter='data')
assert (ROOT/'.github/workflows/build.yml').read_bytes()==(source/'.github/workflows/build.yml').read_bytes(),'Workflow must first be authorized separately'
plain=[
 'src/windows/OverlayWindow.tsx','package.json','src-tauri/tauri.conf.json','src-tauri/Cargo.toml','src-tauri/Cargo.lock',
 'scripts/ocr-selection-commit.test.mjs','docs/architecture.md','docs/adr/0028-ocr-committed-selection.md','docs/releases/v1.4.10.md',
 'docs/demos/full-flow/.gitignore','docs/demos/full-flow/desktop.js','docs/demos/full-flow/record.py','docs/demos/full-flow/package.py','docs/demos/full-flow/test-ocr.py'
]
for name in plain:
 destination=ROOT/name;destination.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(source/name,destination)
deletions=['docs/demos/full-flow/__pycache__/package.cpython-312.pyc','docs/demos/full-flow/__pycache__/record.cpython-312.pyc','docs/demos/capture/record.py','scripts/maintenance/finish-v1.4.10-ocr.py']
for name in deletions:(ROOT/name).unlink(missing_ok=True)
# Check every application/build input, not just modified code, against the recording.
inputs=[p for folder in ['src','src-tauri','public'] for p in (source/folder).rglob('*') if p.is_file()]
inputs += [source/n for n in ['package.json','pnpm-lock.yaml','tsconfig.json','vite.config.ts','index.html'] if (source/n).is_file()]
for file in inputs:
 assert (ROOT/file.relative_to(source)).read_bytes()==file.read_bytes(),str(file)
subprocess.run(['node','scripts/release-version.mjs','v1.4.10'],check=True)
subprocess.run(['git','add','--',*plain],check=True)
subprocess.run(['git','add','-u','--',*deletions],check=True)
subprocess.run(['git','diff','--cached','--check'],check=True)
assert '.github/workflows/' not in subprocess.check_output(['git','diff','--cached','--name-only'],text=True)
subprocess.run(['git','commit','-m','fix: commit OCR only on release and preserve full selection bounds'],check=True)
equivalent=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip()
d=json.loads((review/'provenance.json').read_text())
assert d['checks']['no_recording_cancel_demo'] and d['checks']['ocr_request_count']==1 and d['checks']['ocr_lines']==12
assert d['checks']['ocr_selection']=={'x':194,'y':104,'width':896,'height':537}
assert d['duration']==27.2 and d['ending_hold_seconds']>=6
for name,info in d['media'].items():
 p=review/name;assert len(p.read_bytes())==info['bytes'] and sha(p)==info['sha256'];shutil.copyfile(p,ROOT/'docs/demos'/name)
d.update(verified_equivalent_source_commit=equivalent,source_build_artifact=9991358377,verified_application_input_count=len(inputs),source_archive_sha256=sha(source_tar),source_commit_note='Original source commit was created in the capture runner. The equivalent published source has byte-identical application and build inputs; no new recording is claimed.')
(ROOT/'docs/demos/provenance.json').write_text(json.dumps(d,ensure_ascii=False,indent=2)+'\n')
shutil.copyfile(review/'README.md',ROOT/'docs/demos/README.md')
for name in ['README.md','README_EN.md','README_JA.md','README_ZH.md']:shutil.copyfile(source/name,ROOT/name)
index=ROOT/'docs/README.md';text=index.read_text();assert '0028-ocr-committed-selection' not in text
index.write_text(text+'\n- [ADR 0028: Committed OCR selection](adr/0028-ocr-committed-selection.md)\n')
subprocess.run(['git','add','docs/demos','docs/README.md','README.md','README_EN.md','README_JA.md','README_ZH.md'],check=True)
subprocess.run(['git','diff','--cached','--check'],check=True)
assert '.github/workflows/' not in subprocess.check_output(['git','diff','--cached','--name-only'],text=True)
subprocess.run(['git','commit','-m','docs: use reviewed full-region OCR demo with a six-second ending'],check=True)
print('Imported reviewed source and media. Public source equivalence:',equivalent)
