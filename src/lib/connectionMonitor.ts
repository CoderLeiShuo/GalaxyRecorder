export type ConnectionHealth = { connected: boolean; definitive?: boolean; reason?: string };

// A device response confirms a connection; internet availability alone never does.
export function monitorConnection(options: {
  check: () => Promise<ConnectionHealth>;
  checking: () => void;
  ready: () => void;
  lost: (reason: string) => void;
}) {
  let ended = false; let paused = false; let running = false; let requested = false;
  let generation = 0; let failures = 0; let timer: ReturnType<typeof setTimeout> | undefined;
  const clear = () => { clearTimeout(timer); timer = undefined; };
  function lost(reason = '记录仪已失联，请重新连接车机热点') {
    if (ended) return;
    ended = true; generation++; clear(); options.lost(reason);
  }
  async function run() {
    if (ended || paused || running) return;
    running = true; requested = false; const current = generation;
    let result: ConnectionHealth;
    try { result = await options.check(); }
    catch { result = { connected: false, reason: '无法确认记录仪连接，请重新连接车机热点' }; }
    if (!ended && !paused && current === generation) {
      if (result.connected) { failures = 0; options.ready(); }
      else if (result.definitive || ++failures >= 2) lost(result.reason);
      else options.checking();
    }
    running = false;
    if (ended || paused) return;
    if (requested) void run();
    else timer = setTimeout(() => void run(), failures ? 1000 : 5000);
  }
  function resume() {
    if (ended) return;
    clear(); paused = false; generation++; failures = 0; requested = true;
    options.checking(); void run();
  }
  function suspend() {
    if (ended) return;
    clear(); paused = true; generation++; requested = false; options.checking();
  }
  // The initial discovery has just confirmed the device. Recheck it periodically.
  timer = setTimeout(() => void run(), 5000);
  return { resume, suspend, lost, dispose: () => { ended = true; generation++; clear(); } };
}
