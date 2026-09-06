from pathlib import Path
import json, re, subprocess
ROOT=Path.cwd()

def replace(path,old,new):
    p=ROOT/path
    s=p.read_text(encoding='utf-8')
    assert s.count(old)==1,(path,old,s.count(old))
    p.write_text(s.replace(old,new),encoding='utf-8')

replace('package.json','"version": "1.4.10"','"version": "1.4.11"')
replace('src-tauri/tauri.conf.json','"version": "1.4.10"','"version": "1.4.11"')
replace('src-tauri/Cargo.toml','version = "1.4.10"','version = "1.4.11"')
replace('src-tauri/Cargo.lock','name = "kiri"\nversion = "1.4.10"','name = "kiri"\nversion = "1.4.11"')
# Documentation harness version should match the product release it demonstrates.
p=ROOT/'docs/demos/full-flow/desktop.js'
if p.exists():
    s=p.read_text(encoding='utf-8')
    if "if(c==='plugin:app|version')return '1.4.10';" in s:
        p.write_text(s.replace("if(c==='plugin:app|version')return '1.4.10';","if(c==='plugin:app|version')return '1.4.11';"),encoding='utf-8')
notes=ROOT/'docs/releases/v1.4.11.md'
assert not notes.exists()
notes.write_text('''# Kiri v1.4.11

## 修复

- 修复 macOS 在浏览器或播放器进入原生全屏后，按 Kiri 全局快捷键时截图层可能出现在其他 Space、看起来像没有触发的问题。
- 截图层现在会留在当前全屏 Space 上方显示，不再为了显示截图层切换回 Kiri；点击截图层后仍可正常交互。
- Windows 继续使用原有的置顶 + 聚焦路径；本次没有发现同类问题，并通过 Windows 回归、编译和实际桌面录制控制检查。

## 继续包含 v1.4.10 的改进

- OCR 等待鼠标松开后再按完整选区识别，避免拖选中途提前识别小框。
- 紧凑黑色录屏倒计时与录制控制状态同步修复。
- 完整演示更新：更大的 OCR 区域、更丰富的识别内容和更长的结尾停留。

Kiri 仍然不做后台更新检查或静默安装；更新由用户在设置中主动触发。
''',encoding='utf-8')
subprocess.run(['node','scripts/release-version.mjs','v1.4.11'],check=True)
subprocess.run(['git','diff','--check'],check=True)
print('v1.4.11 metadata prepared')
