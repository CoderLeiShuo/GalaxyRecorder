"""Keep only the app's own disassembly from the Android SDK dexdump tool."""
import subprocess
import sys
import zipfile
import json
import re
from pathlib import Path

output = Path(sys.argv[2])
dex = output.parent / 'classes.dex'
with zipfile.ZipFile(sys.argv[1]) as archive:
    dex.write_bytes(archive.read('classes.dex'))
command = [r'D:\Android\SDK\build-tools\36.1.0\dexdump.exe', '-d', str(dex)]
process = subprocess.Popen(command, stdout=subprocess.PIPE, stderr=subprocess.PIPE, encoding='utf-8', errors='replace')
block = []
kept = 0
with output.open('w', encoding='utf-8') as target:
    for line in process.stdout:
        if line.startswith('Class #'):
            if any("Class descriptor  : 'Lcom/galaxy/" in text for text in block[:8]):
                target.writelines(block)
                kept += 1
            block = []
        block.append(line)
    if any("Class descriptor  : 'Lcom/galaxy/" in text for text in block[:8]):
        target.writelines(block)
        kept += 1
error = process.stderr.read()
code = process.wait()
dex.unlink()
print(f'exit={code}; app classes={kept}; bytes={output.stat().st_size}')
methods = []
current = None
for number, line in enumerate(output.read_text(encoding='utf-8').splitlines(), 1):
    match = re.search(r'\|\[[0-9a-f]+\] (com\.galaxy\.[^:]+):', line)
    if match:
        current = {'method': match[1], 'line': number, 'strings': [], 'calls': []}
        methods.append(current)
    elif current:
        match = re.search(r'const-string(?:/jumbo)? .*?, "(.*)" // string@', line)
        if match and match[1] not in current['strings']:
            current['strings'].append(match[1])
        match = re.search(r'invoke-[^ ]+ .*?\}, (.*?) // method@', line)
        if match and match[1] not in current['calls']:
            current['calls'].append(match[1])
output.with_name('verified-methods.json').write_text(json.dumps(methods, ensure_ascii=False, indent=2), encoding='utf-8')
if error:
    print(error[:1000])
sys.exit(code)
