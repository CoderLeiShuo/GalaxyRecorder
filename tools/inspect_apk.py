"""Read APK metadata and DEX evidence without executing application code."""
import hashlib
import json
import re
import struct
import sys
import zipfile
from pathlib import Path


def uleb(data, offset):
    value = shift = 0
    while True:
        byte = data[offset]
        offset += 1
        value |= (byte & 127) << shift
        if byte < 128:
            return value, offset
        shift += 7


def inspect(source, target):
    target.mkdir(parents=True, exist_ok=True)
    raw = source.read_bytes()
    archive = zipfile.ZipFile(source)
    data = archive.read('classes.dex')
    count, offset = struct.unpack_from('<II', data, 56)
    strings = []
    for index in range(count):
        pointer = struct.unpack_from('<I', data, offset + index * 4)[0]
        _, pointer = uleb(data, pointer)
        strings.append(data[pointer:data.index(b'\0', pointer)].decode('utf-8', 'replace'))
    count, offset = struct.unpack_from('<II', data, 64)
    types = [strings[struct.unpack_from('<I', data, offset + i * 4)[0]] for i in range(count)]
    count, offset = struct.unpack_from('<II', data, 88)
    methods = []
    for i in range(count):
        class_index, proto_index, name_index = struct.unpack_from('<HHI', data, offset + i * 8)
        methods.append({'class': types[class_index], 'name': strings[name_index]})
    count, offset = struct.unpack_from('<II', data, 96)
    classes = []
    for i in range(count):
        fields = struct.unpack_from('<8I', data, offset + i * 32)
        class_name = types[fields[0]]
        if not class_name.startswith('Lcom/galaxy/'):
            continue
        item = {'class': class_name, 'super': types[fields[2]], 'methods': []}
        pointer = fields[6]
        if pointer:
            sizes = []
            for _ in range(4):
                size, pointer = uleb(data, pointer)
                sizes.append(size)
            for _ in range(sizes[0] + sizes[1]):
                _, pointer = uleb(data, pointer)
                _, pointer = uleb(data, pointer)
            for size in sizes[2:]:
                method_index = 0
                for _ in range(size):
                    delta, pointer = uleb(data, pointer)
                    method_index += delta
                    _, pointer = uleb(data, pointer)
                    code_offset, pointer = uleb(data, pointer)
                    method = dict(methods[method_index])
                    method['code_offset'] = code_offset
                    method['strings'] = []
                    if code_offset:
                        units = struct.unpack_from('<I', data, code_offset + 12)[0]
                        code = data[code_offset + 16:code_offset + 16 + units * 2]
                        # Candidate const-string references. Full disassembly should
                        # validate instructions before relying on a particular edge.
                        seen = set()
                        for pos in range(0, len(code) - 3, 2):
                            if code[pos] not in (0x1a, 0x1b):
                                continue
                            width = 2 if code[pos] == 0x1a else 4
                            if pos + 2 + width > len(code):
                                continue
                            string_index = int.from_bytes(code[pos + 2:pos + 2 + width], 'little')
                            if string_index < len(strings) and string_index not in seen:
                                method['strings'].append(strings[string_index])
                                seen.add(string_index)
                    item['methods'].append(method)
        classes.append(item)
    ui_strings = [s for s in strings if len(s) < 1200 and re.search('[\u4e00-\u9fff]', s) and not s.startswith(('(?', '[', '\\'))]
    urls = [s for s in strings if len(s) < 1200 and re.search(r'https?://|rtsp://|192\.168\.|cgi-bin|\.cgi\b|\.jsp\b', s)]
    metadata = {'apk': str(source), 'bytes': len(raw), 'sha256': hashlib.sha256(raw).hexdigest(),
                'dex_strings': len(strings), 'app_classes': len(classes), 'files': archive.namelist(),
                'vehicles': json.loads(archive.read('assets/vehicle_names.json'))}
    for name, value in [('metadata.json', metadata), ('candidate-string-references.json', classes), ('urls.json', urls)]:
        (target / name).write_text(json.dumps(value, ensure_ascii=False, indent=2), encoding='utf-8')
    (target / 'ui-strings.txt').write_text('\n'.join(ui_strings), encoding='utf-8')
    (target / 'dex-strings.txt').write_text('\n'.join(strings), encoding='utf-8')
    (target / 'class-methods.txt').write_text('\n'.join(c['class'] + '\n' + '\n'.join('  ' + m['name'] for m in c['methods']) for c in classes), encoding='utf-8')
    print(json.dumps(metadata, ensure_ascii=False, indent=2)[:2200])
    print('\nAPP CLASSES\n' + '\n'.join(c['class'] for c in classes if '$' not in c['class']))


if __name__ == '__main__':
    inspect(Path(sys.argv[1]), Path(sys.argv[2]))
