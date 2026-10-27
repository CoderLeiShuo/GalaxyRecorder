"""Print exact app instructions for selected method suffixes."""
import json
import sys
from pathlib import Path

methods = json.loads(Path('analysis/verified-methods.json').read_text(encoding='utf-8'))
lines = Path('analysis/app-disassembly.txt').read_text(encoding='utf-8').splitlines()
for index, method in enumerate(methods):
    if any(method['method'].endswith(suffix) for suffix in sys.argv[1:]):
        end = methods[index + 1]['line'] - 1 if index + 1 < len(methods) else len(lines)
        print('\n' + method['method'] + ' @' + str(method['line']))
        print('\n'.join(line.split('|', 1)[-1] for line in lines[method['line'] - 1:end] if '|' in line))
