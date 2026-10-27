"""Capture only this app while it is the foreground activity."""
import subprocess
import sys
import time
from pathlib import Path

adb = r'D:\Android\SDK\platform-tools\adb.exe'
serial, destination = sys.argv[1:3]
prefix = [adb, '-s', serial]
time.sleep(1)
state = subprocess.run(prefix + ['shell', 'dumpsys', 'window'], capture_output=True, text=True, encoding='utf-8', errors='replace', check=True).stdout
focus = [line for line in state.splitlines() if 'mCurrentFocus=' in line]
if not any('com.leishuo.galaxyrecorder' in line for line in focus):
    raise SystemExit('目标应用未在前台，未截取其他应用的画面。')
result = subprocess.run(prefix + ['exec-out', 'screencap', '-p'], capture_output=True, check=True)
if not result.stdout.startswith(b'\x89PNG\r\n\x1a\n'):
    raise SystemExit('手机未返回有效截图。')
target = Path(destination)
target.parent.mkdir(parents=True, exist_ok=True)
target.write_bytes(result.stdout)
print(f'App screenshot saved: {target} ({len(result.stdout)} bytes)')
