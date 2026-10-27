const { chromium } = require('C:/Users/CoderLeiShuo/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/playwright');
const fs = require('fs');

// Exercises native command wiring with a simulated bridge; does not contact a LAN.
(async () => {
  const browser = await chromium.launch({ headless: true, executablePath: 'C:/Program Files/Google/Chrome/Application/chrome.exe' });
  const page = await browser.newPage({ viewport: { width: 360, height: 792 } });
  const errors = [];
  page.on('pageerror', error => errors.push(error.message));
  await page.addInitScript(() => {
    globalThis.isTauri = true;
    let index = 0;
    const callbacks = new Map();
    const listeners = new Map();
    const link = { id: 'test-network', interfaceName: 'wlan0', ip: '192.168.42.20', prefixLength: 23, gateway: '192.168.42.1' };
    const network = { platform: 'android', available: true, selected: link, networks: [link], bound: true, defaultTransport: 'cellular', message: '模拟网络' };
    const mock = globalThis.connectionMock = { calls: [], mode: 'cancel', pending: null, diagnostics: { method: 'none', result: 'idle', network: null, checked: 0, total: 0, lastError: null, events: [] } };
    globalThis.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: (_, id) => listeners.delete(id) };
    globalThis.__TAURI_INTERNALS__ = {
      transformCallback: callback => { callbacks.set(++index, callback); return index; },
      unregisterCallback: id => callbacks.delete(id),
      invoke: async (command, args = {}) => {
        mock.calls.push({ command, args });
        if (command === 'plugin:event|listen') { const id = ++index; listeners.set(id, { event: args.event, handler: args.handler }); return id; }
        if (command === 'plugin:event|unlisten') return;
        if (command === 'list_saved') return [];
        if (command === 'get_recorder_network') return { ...network, bound: false };
        if (command === 'get_connection_diagnostics') return mock.diagnostics;
        if (command === 'check_recorder_network') {
          mock.diagnostics = { ...mock.diagnostics, method: 'network-check', result: 'success', network, events: [{ elapsedMs: 10, stage: 'network-check', message: '自检未发送设备请求' }] };
          return network;
        }
        if (command === 'cancel_recorder_discovery') { if (mock.pending) { mock.pending('查找已取消'); mock.pending = null; } return; }
        if (command === 'release_recorder_network') return;
        if (command === 'discover_recorder') {
          mock.diagnostics = { ...mock.diagnostics, method: 'automatic', result: 'failed', lastError: '模拟设备未响应', checked: 16, total: 509 };
          for (const [id, listener] of listeners) if (listener.event === 'connection-progress') callbacks.get(listener.handler)({ id, event: listener.event, payload: { checked: 16, total: 509, message: '正在查找记录仪，已检查 16/509 个地址…' } });
          if (mock.mode === 'cancel') return new Promise((_, reject) => { mock.pending = reject; });
          if (mock.mode === 'failure') throw '模拟设备未响应';
          return { address: '192.168.43.8', protocol: 'A7 / Config.cgi', firmware: '待进一步读取', vehicle: '2026 款星越 L', discoveredVia: 'UDP 49142' };
        }
        if (command === 'list_videos') return { videos: [], warning: null };
        throw new Error('Unexpected native command: ' + command);
      }
    };
  });
  await page.goto('http://127.0.0.1:1420');
  await page.locator('.glacier-connect-button').click();
  const dialog = page.getByRole('dialog', { name: '连接记录仪' });
  await dialog.getByText('已检测到 Wi-Fi', { exact: true }).waitFor();
  if (await dialog.getByRole('textbox', { name: '记录仪地址' }).isVisible()) throw Error('Manual input must be collapsed on mobile');
  await dialog.getByRole('button', { name: '网络自检', exact: true }).click();
  await dialog.getByText('Wi-Fi 自检通过，应用可使用此网络。自检未查找记录仪。', { exact: true }).waitFor();
  await dialog.getByRole('button', { name: '自动查找记录仪', exact: true }).click();
  await dialog.getByText('正在查找记录仪，已检查 16/509 个地址…', { exact: true }).waitFor();
  await dialog.getByRole('button', { name: '停止查找', exact: true }).click();
  await dialog.getByRole('alert').filter({ hasText: '查找已取消' }).waitFor();
  await page.evaluate(() => { connectionMock.mode = 'failure'; });
  await dialog.getByRole('button', { name: '自动查找记录仪', exact: true }).click();
  await dialog.getByRole('alert').filter({ hasText: '模拟设备未响应' }).waitFor();
  await dialog.getByRole('button', { name: '连接诊断', exact: true }).click();
  await dialog.getByRole('textbox', { name: '诊断内容' }).waitFor();
  const report = JSON.parse(await dialog.getByRole('textbox', { name: '诊断内容' }).inputValue());
  if (report.network.defaultTransport !== 'cellular' || report.network.bound !== true) throw Error('Diagnostics lost Wi-Fi routing state');
  await dialog.screenshot({ path: 'analysis/connection-dialog-simulated.png' });
  await page.evaluate(() => { connectionMock.mode = 'success'; });
  await dialog.getByRole('button', { name: '自动查找记录仪', exact: true }).click();
  await dialog.waitFor({ state: 'detached' });
  await page.locator('.glacier-recorder-strip').getByText('已连接', { exact: true }).waitFor();
  const calls = await page.evaluate(() => connectionMock.calls);
  const automatic = calls.filter(call => call.command === 'discover_recorder');
  if (automatic.length !== 3 || automatic.some(call => call.args.preferred !== null || call.args.subnet !== undefined)) throw Error('Auto discovery still depends on typed IP');
  if (calls.filter(call => call.command === 'list_videos').length !== 2) throw Error('Successful discovery did not load both categories');
  await page.getByRole('button', { name: '管理记录仪连接', exact: true }).click();
  await page.getByRole('button', { name: '断开当前连接', exact: true }).click();
  await page.waitForFunction(() => connectionMock.calls.some(call => call.command === 'release_recorder_network'));
  await browser.close();
  const checks = ['automatic discovery without IP', 'Wi-Fi-only self check', 'progress event', 'cancel native discovery', 'failure diagnostics', 'connect and load categories', 'disconnect releases network'];
  fs.writeFileSync('analysis/connection-ui-verification.json', JSON.stringify({ simulatedBridge: true, liveVehicle: false, errors, checks }, null, 2));
  console.log(JSON.stringify({ simulatedBridge: true, checks: checks.length, errors }));
  if (errors.length) process.exitCode = 1;
})().catch(error => { console.error(error); process.exit(1); });
