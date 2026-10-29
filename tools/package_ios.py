#!/usr/bin/env python3
"""Build/package the unsigned iPhoneOS app; never signs or installs on a device."""
import argparse
import hashlib
import json
from pathlib import Path
import plistlib
import shutil
import subprocess
import zipfile

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--app', type=Path, help='Package an existing unsigned iPhoneOS .app')
    args = parser.parse_args()
    if args.app:
        app = args.app.resolve()
    else:
        icon_set = ROOT / 'src-tauri/gen/apple/Assets.xcassets/AppIcon.appiconset'
        if not icon_set.is_dir():
            raise SystemExit('Initialize the Xcode project first: npm run ios:init')
        for icon in (ROOT / 'src-tauri/icons/ios').glob('*.png'):
            shutil.copy2(icon, icon_set / icon.name)
        subprocess.run(['npm', 'run', 'tauri', '--', 'ios', 'build', '--ci', '--no-sign', '--archive-only', '--target', 'aarch64'], cwd=ROOT, check=True)
        apps = list((ROOT / 'src-tauri/gen/apple/build').glob('**/*.xcarchive/Products/Applications/*.app'))
        if not apps:
            raise SystemExit('No iPhoneOS archive found; pass --app /path/to/unsigned.app')
        app = max(apps, key=lambda p: p.stat().st_mtime)
    with (app / 'Info.plist').open('rb') as f:
        info = plistlib.load(f)
    assert 'iPhoneOS' in info.get('CFBundleSupportedPlatforms', []), 'Simulator builds cannot become device IPAs'
    assert info['CFBundleIdentifier'] == 'com.leishuo.galaxyrecorder'
    assert all(info.get(key) for key in ['NSLocalNetworkUsageDescription', 'NSPhotoLibraryUsageDescription', 'NSPhotoLibraryAddUsageDescription'])
    executable = app / info['CFBundleExecutable']
    arch = subprocess.check_output(['lipo', '-archs', str(executable)], text=True).strip()
    assert arch == 'arm64', arch
    assert not (app / 'embedded.mobileprovision').exists(), 'Expected no provisioning profile'
    assert not (app / '_CodeSignature').exists(), 'Expected an unsigned build'
    version = info['CFBundleShortVersionString']
    output = ROOT / 'output' / ('jixing-yingji-' + version + '-ios-arm64-unsigned.ipa')
    output.parent.mkdir(exist_ok=True)
    temporary = output.with_suffix('.ipa.part')
    with zipfile.ZipFile(temporary, 'w', compression=zipfile.ZIP_DEFLATED, compresslevel=9) as package:
        for path in sorted(app.rglob('*')):
            if path.is_file():
                package.write(path, str(Path('Payload') / app.name / path.relative_to(app)))
    with zipfile.ZipFile(temporary) as package:
        assert package.testzip() is None, 'IPA zip integrity check failed'
    temporary.replace(output)
    metadata = {
        'application': info.get('CFBundleDisplayName', info['CFBundleName']),
        'version': version, 'build': info['CFBundleVersion'], 'bundleId': info['CFBundleIdentifier'],
        'platform': 'iPhoneOS', 'architecture': arch, 'minimumIOS': info['MinimumOSVersion'],
        'signed': False, 'requiresUserSigning': True, 'bytes': output.stat().st_size,
        'sha256': hashlib.sha256(output.read_bytes()).hexdigest(), 'file': output.name,
    }
    output.with_suffix('.json').write_text(json.dumps(metadata, ensure_ascii=False, indent=2) + '\n')
    print(json.dumps(metadata, ensure_ascii=False, indent=2))


if __name__ == '__main__':
    main()
