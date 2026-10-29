const { chromium } = require(process.env.PLAYWRIGHT_MODULE || 'playwright');
const assert = require('node:assert/strict');
const fs = require('node:fs');
(async () => {
  const browser = await chromium.launch({ headless: true, executablePath: process.env.CHROME_PATH || '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome' });
  try {
    const page = await browser.newPage({ viewport: { width: 390, height: 844 }, userAgent: 'Mozilla/5.0 (Linux; Android 14) AppleWebKit/537.36 Chrome/140.0.0.0 Mobile Safari/537.36' });
    const errors = []; const checks = []; page.on('pageerror', error => errors.push(error.message));
    await page.addInitScript(() => {
      globalThis.isTauri = true; let next = 0; const callbacks = new Map(); const listeners = new Map();
      const base = { name: '20261028_080000.mp4', path: '/mnt/mmc/Normal/20261028_080000.mp4', category: 'normal', date: '2026-10-28', time: '08:00:00', size: 1048576, duration: 12 };
      const mock = globalThis.workflowMock = { calls: [], saved: [{ ...base, id: 'saved:1', savedPath: '/fixture/one.mp4' }, { ...base, id: 'saved:2', time: '07:00:00', savedPath: '/fixture/two.mp4' }], tasks: [], fail: 'saved:2', hold: false, finish: null, copied: '', settings: { saveMode: 'gallery', directory: null, directoryLabel: null, maskByDefault: false, maskDownloads: false, keepScreenOn: true },
        emit: (event, payload) => { for (const [id, listener] of listeners) if (listener.event === event) callbacks.get(listener.handler)?.({ id, event, payload }); } };
      Object.defineProperty(navigator, 'clipboard', { configurable: true, value: { writeText: async value => { mock.copied = value; } } });
      globalThis.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: (_, id) => listeners.delete(id) };
      globalThis.__TAURI_INTERNALS__ = { transformCallback: callback => { callbacks.set(++next, callback); return next; }, unregisterCallback: id => callbacks.delete(id), invoke: async (command, args = {}) => {
        mock.calls.push({ command, args });
        if (command === 'plugin:event|listen') { const id = ++next; listeners.set(id, args); return id; }
        if (command === 'plugin:event|unlisten' || command === 'cancel_recorder_discovery' || command === 'record_client_event') return;
        if (command === 'list_saved') return mock.saved;
        if (command === 'list_transfers') return mock.tasks;
        if (command === 'get_settings') return mock.settings;
        if (command === 'save_settings') return mock.settings = args.settings;
        if (command === 'choose_download_directory') return mock.settings = { ...mock.settings, saveMode: 'folder', directory: 'content://fixture/tree', directoryLabel: '批量导出目录' };
        if (command === 'get_recorder_network') return { available: true, bound: true, platform: 'android', selected: { id: 'car', ip: '192.168.43.20', prefixLength: 24 } };
        if (command === 'discover_recorder') return { address: '192.168.43.8', networkId: 'car', protocol: 'A7', firmware: 'test', vehicle: '2026 款星越 L' };
        if (command === 'check_recorder_connection') return { connected: true };
        if (command === 'get_connection_diagnostics') return { events: [] };
        if (command === 'list_videos') return { videos: [], warning: null };
        if (command === 'video_metadata') return { duration: 12, thumbnail: '/fixture-thumbnail.png', warning: null };
        if (command === 'share_saved_many') return;
        if (command === 'export_saved') {
          const result = { ...mock.saved.find(video => video.id === args.id), exportedPath: 'content://fixture/exported' };
          if (mock.hold) return new Promise(resolve => { mock.finish = () => resolve(result); });
          if (args.id === mock.fail) throw '模拟空间不足';
          return result;
        }
        if (command === 'recorder_status') return { fields: { Sd: '0', TotalSize: '68719476736', usedNormalSpace: '25769803776', usedEventSpace: '8589934592', usedPhotoSpace: '0', FreeSize: '34359738368', Record: '2', mic: 'ON' }, warnings: [] };
        if (command === 'get_operation_logs') return JSON.stringify({ operations: [{ category: 'download', message: '本地日志测试' }] }, null, 2);
        if (command === 'export_operation_logs') return true;
        if (command === 'control_transfers') { mock.tasks = args.action === 'clearHistory' ? mock.tasks.filter(task => ['waiting','downloading'].includes(task.status)) : mock.tasks.map(task => ['waiting','downloading'].includes(task.status) ? { ...task, status: 'cancelled' } : task); return mock.tasks; }
        throw Error('Unexpected mock command: ' + command);
      } };
    });
    await page.route('**/fixture-thumbnail.png',route=>route.fulfill({path:'tools/fixtures/thumbnail.png',contentType:'image/png'}));
 await page.goto('http://127.0.0.1:1420'); await page.locator('.glacier-connection-state.online').waitFor(); await page.locator('.glacier-connect-button').click();
    await page.locator('.glacier-recorder-strip').getByText('已连接', { exact: true }).waitFor();
    await page.locator('.filter-tabs button').filter({ hasText: '已保存' }).click();
    await page.getByRole('button', { name: '批量选择', exact: true }).click();
    await page.locator('.video-image').nth(0).click(); await page.locator('.video-image').nth(1).click();
    await page.getByRole('button', { name: '导出与分享', exact: true }).click();
    const batch = page.getByRole('dialog', { name: '批量导出与分享', exact: true });
    await batch.getByRole('button', { name: '批量分享视频', exact: true }).click();
    let call = await page.evaluate(() => workflowMock.calls.find(call => call.command === 'share_saved_many'));
    assert.deepEqual(call.args.ids.sort(), ['saved:1','saved:2']); assert.equal(call.args.original, false); checks.push('batch video sharing');
    await batch.getByRole('button', { name: '以原文件分享', exact: true }).click();
    call = await page.evaluate(() => workflowMock.calls.filter(call => call.command === 'share_saved_many').at(-1));
    assert.equal(call.args.original, true); checks.push('original file sharing');
    await batch.getByRole('button', { name: '导出到指定目录', exact: true }).click();
    await batch.getByText('已导出 1 段，失败 1 段', { exact: true }).waitFor();
    await batch.getByRole('alert').filter({ hasText: '模拟空间不足' }).waitFor(); checks.push('folder export reports partial success');
    await batch.screenshot({ path: 'analysis/batch-actions-0.3.0-mobile.png' });
    const before = await page.evaluate(() => workflowMock.calls.filter(call => call.command === 'export_saved').length);
    await page.evaluate(() => { workflowMock.hold = true; workflowMock.fail = null; });
    await batch.getByRole('button', { name: '批量保存到相册', exact: true }).click(); await page.waitForFunction(() => Boolean(workflowMock.finish));
    await batch.getByRole('button', { name: '停止后续导出', exact: true }).click(); await page.evaluate(() => workflowMock.finish());
    await batch.getByText('已导出 1 段，已停止后续导出', { exact: true }).waitFor();
    assert.equal(await page.evaluate(() => workflowMock.calls.filter(call => call.command === 'export_saved').length), before + 1); checks.push('stops after the current export');
    await batch.getByRole('button', { name: '关闭批量操作', exact: true }).click();
    await page.locator('.nav-item').filter({ hasText: '设置与帮助' }).click();
    await page.getByRole('checkbox', { name: '下载时遮挡车架号', exact: true }).check();
    assert.equal(await page.evaluate(() => workflowMock.settings.maskDownloads), true); checks.push('download masking preference persists');
    await page.getByRole('img', { name: '已用 50%', exact: true }).waitFor();
    await page.getByText('正在录制紧急录像', { exact: true }).waitFor(); checks.push('storage category usage and emergency recording state');
    await page.getByRole('button', { name: '查看运行日志', exact: true }).click();
    const logs = page.getByRole('dialog', { name: '运行日志', exact: true });
    await page.waitForFunction(() => document.querySelector('textarea[aria-label="运行日志内容"]')?.value.includes('本地日志测试'));
    await logs.getByRole('button', { name: '复制日志', exact: true }).click();
    assert.ok((await page.evaluate(() => workflowMock.copied)).includes('本地日志测试')); checks.push('view and copy persisted operation logs');
    await logs.getByRole('button', { name: '分享日志', exact: true }).click();
    await logs.getByRole('button', { name: '导出日志文件', exact: true }).click();
    assert.deepEqual(await page.evaluate(() => workflowMock.calls.filter(call => call.command === 'export_operation_logs').map(call => call.args.share)), [true,false]); checks.push('log file sharing and saving');
    await logs.getByRole('button', { name: '关闭运行日志', exact: true }).click();
    await page.evaluate(() => { workflowMock.tasks = [{ id: 'job', video: workflowMock.saved[0], status: 'downloading', received: 1048576, total: 1048576, maskVin: true, phase: 'masking' }]; workflowMock.emit('transfer-state',workflowMock.tasks); });
    await page.waitForFunction(() => [...document.querySelectorAll('button')].find(button => button.textContent.trim() === '格式化')?.disabled);
    assert.equal(await page.getByRole('button', { name: '格式化', exact: true }).isEnabled(), false); checks.push('format blocked during active downloads');
    await page.locator('.nav-item').filter({ hasText: '传输任务' }).click();
    await page.getByText('正在打码 · 打码保存', { exact: true }).waitFor(); checks.push('masking phase in transfer queue');
    await page.getByRole('button', { name: '取消全部下载', exact: true }).click();
    await page.getByText('已取消 · 打码保存', { exact: true }).waitFor(); checks.push('cancel all downloads');
    await page.getByRole('button', { name: '清理传输记录', exact: true }).click();
    await page.getByText('暂时没有传输任务', { exact: true }).waitFor();
    assert.equal(await page.evaluate(() => workflowMock.saved.length), 2); checks.push('history cleanup preserves saved videos');
    assert.equal(await page.evaluate(() => document.documentElement.scrollWidth > innerWidth), false); checks.push('mobile workflow layout');
    assert.deepEqual(errors, []);
    fs.writeFileSync('analysis/workflows-ui-verification.json', JSON.stringify({ simulatedBridge: true, liveVehicle: false, checks, errors }, null, 2));
    console.log(JSON.stringify({ passed: checks.length, checks, errors }));
  } finally { await browser.close(); }
})().catch(error => { console.error(error); process.exit(1); });
