import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { monitorConnection, type ConnectionHealth } from './connectionMonitor';

describe('recorder connection lifetime', () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());
  function setup(check = vi.fn<() => Promise<ConnectionHealth>>().mockResolvedValue({ connected: true })) {
    const callbacks = { checking: vi.fn(), ready: vi.fn(), lost: vi.fn(), check };
    return { ...callbacks, monitor: monitorConnection(callbacks) };
  }
  it('checks device reachability periodically', async () => {
    const test = setup();
    await vi.advanceTimersByTimeAsync(5000);
    expect(test.check).toHaveBeenCalledTimes(1); expect(test.ready).toHaveBeenCalledOnce();
    test.monitor.dispose();
  });
  it('marks a suspended session unconfirmed and verifies it on return', async () => {
    const test = setup(); test.monitor.suspend();
    expect(test.checking).toHaveBeenCalledOnce();
    await vi.advanceTimersByTimeAsync(15000); expect(test.check).not.toHaveBeenCalled();
    test.monitor.resume(); await vi.advanceTimersByTimeAsync(0);
    expect(test.check).toHaveBeenCalledOnce(); expect(test.ready).toHaveBeenCalledOnce(); test.monitor.dispose();
  });
  it('clears a lost Wi-Fi session immediately and rejects late success', async () => {
    let finish!: (value: ConnectionHealth) => void;
    const test = setup(vi.fn(() => new Promise(resolve => { finish = resolve; })));
    test.monitor.resume(); test.monitor.lost('Wi-Fi 已断开'); finish({ connected: true });
    await vi.advanceTimersByTimeAsync(0);
    expect(test.lost).toHaveBeenCalledWith('Wi-Fi 已断开'); expect(test.ready).not.toHaveBeenCalled();
  });
  it('does not retain connected status across two failed device checks', async () => {
    const test = setup(vi.fn<() => Promise<ConnectionHealth>>().mockResolvedValue({ connected: false }));
    await vi.advanceTimersByTimeAsync(5000); expect(test.checking).toHaveBeenCalledOnce(); expect(test.lost).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(1000); expect(test.lost).toHaveBeenCalledOnce();
    await vi.advanceTimersByTimeAsync(20000); expect(test.check).toHaveBeenCalledTimes(2);
  });
  it('recovers after a single transient timeout', async () => {
    const check = vi.fn<() => Promise<ConnectionHealth>>().mockResolvedValueOnce({ connected: false }).mockResolvedValue({ connected: true });
    const test = setup(check); await vi.advanceTimersByTimeAsync(6000);
    expect(test.checking).toHaveBeenCalledOnce(); expect(test.ready).toHaveBeenCalledOnce(); expect(test.lost).not.toHaveBeenCalled(); test.monitor.dispose();
  });
  it('disconnects immediately when the OS confirms a changed network', async () => {
    const test = setup(vi.fn<() => Promise<ConnectionHealth>>().mockResolvedValue({ connected: false, definitive: true, reason: 'Wi-Fi 已切换' }));
    test.monitor.resume(); await vi.advanceTimersByTimeAsync(0);
    expect(test.lost).toHaveBeenCalledWith('Wi-Fi 已切换');
  });
  it('ignores a response from before suspend and serializes the new check', async () => {
    let finish!: (value: ConnectionHealth) => void;
    const check = vi.fn<() => Promise<ConnectionHealth>>().mockImplementationOnce(() => new Promise(resolve => { finish = resolve; })).mockResolvedValue({ connected: false, definitive: true });
    const test = setup(check); test.monitor.resume(); test.monitor.suspend(); test.monitor.resume();
    expect(check).toHaveBeenCalledOnce(); finish({ connected: true }); await vi.advanceTimersByTimeAsync(0);
    expect(check).toHaveBeenCalledTimes(2); expect(test.ready).not.toHaveBeenCalled(); expect(test.lost).toHaveBeenCalledOnce();
  });
  it('stops updates from a disposed session after reconnecting or unmounting', async () => {
    let finish!: (value: ConnectionHealth) => void;
    const test = setup(vi.fn(() => new Promise(resolve => { finish = resolve; })));
    test.monitor.resume(); test.monitor.dispose(); finish({ connected: false, definitive: true });
    await vi.advanceTimersByTimeAsync(20000); expect(test.lost).not.toHaveBeenCalled(); expect(test.check).toHaveBeenCalledOnce();
  });
});
