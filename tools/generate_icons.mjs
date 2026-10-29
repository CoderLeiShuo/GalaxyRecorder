import { execFileSync } from 'node:child_process';
import { cpSync, mkdtempSync, mkdirSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('../', import.meta.url));
const output = mkdtempSync(join(tmpdir(), 'jixing-icons-'));
try {
  // An explicit output avoids the CLI choosing only the initialized Android
  // project and leaving the checked-in icon source directory stale.
  execFileSync(resolve(root, 'node_modules/.bin/tauri'), ['icon', 'design/app-icon-v4/icons.json', '-o', output], { cwd: root, stdio: 'inherit' });
  const android = join(output, 'android');
  const adaptive = '<?xml version="1.0" encoding="utf-8"?>\n<adaptive-icon xmlns:android="http://schemas.android.com/apk/res/android">\n  <foreground android:drawable="@mipmap/ic_launcher_foreground"/>\n  <background android:drawable="@mipmap/ic_launcher_background"/>\n</adaptive-icon>\n';
  mkdirSync(join(android, 'mipmap-anydpi-v26'), { recursive: true });
  for (const name of ['ic_launcher.xml', 'ic_launcher_round.xml']) writeFileSync(join(android, 'mipmap-anydpi-v26', name), adaptive);
  cpSync(output, join(root, 'src-tauri/icons'), { recursive: true });
  cpSync(android, join(root, 'src-tauri/gen/android/app/src/main/res'), { recursive: true });
  console.log('Updated platform icons and Android resources from design/app-icon-v4/master.png');
} finally {
  rmSync(output, { recursive: true, force: true });
}
