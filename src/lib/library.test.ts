import {recordingFixtures} from './recordingFixtures.test-data';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { readVideoLibrary } from './recorder';

vi.mock('@tauri-apps/api/core', () => ({
  isTauri: () => true,
  invoke: vi.fn(),
  convertFileSrc: (path: string) => path,
}));

describe('reading the recorder library', () => {
  beforeEach(() => { vi.mocked(invoke).mockReset(); });
  it('waits for the normal listing before requesting events', async () => {
    const calls: string[] = [];
    let finishNormal!: (value: unknown) => void;
    vi.mocked(invoke).mockImplementation((_command, args) => {
      const category = (args as { category: string }).category;
      calls.push(category);
      if (category === 'normal') return new Promise(resolve => { finishNormal = resolve; }) as ReturnType<typeof invoke>;
      return Promise.resolve({ videos: [recordingFixtures[2]], warning: null }) as ReturnType<typeof invoke>;
    });
    const pending = readVideoLibrary('192.168.43.8');
    expect(calls).toEqual(['normal']);
    finishNormal({ videos: [recordingFixtures[0]], warning: null });
    const result = await pending;
    expect(calls).toEqual(['normal', 'event']);
    expect(result.videos).toHaveLength(2);
    expect(result.warnings).toEqual([]);
  });

  it('retains ordinary videos when events fail and identifies the failed category', async () => {
    vi.mocked(invoke).mockResolvedValueOnce({ videos: [recordingFixtures[0]], warning: '保留已读取数据' }).mockRejectedValueOnce('HTTP 500');
    const result = await readVideoLibrary('192.168.43.8');
    expect(result.videos).toEqual([recordingFixtures[0]]);
    expect(result.failed).toEqual(['event']);
    expect(result.warnings).toEqual(['普通录像：保留已读取数据', '紧急录像：HTTP 500']);
  });

  it('stops requesting the old device after the connection changes', async () => {
    let current = true;
    vi.mocked(invoke).mockImplementationOnce(async () => {
      current = false;
      return { videos: [], warning: null };
    });
    const result = await readVideoLibrary('192.168.43.8', () => current);
    expect(result.failed).toEqual([]);
    expect(invoke).toHaveBeenCalledTimes(1);
    expect(vi.mocked(invoke).mock.calls.at(-1)?.[1]).toEqual({ address: '192.168.43.8', category: 'normal' });
  });

  it('handles a large recorder index without exceeding the JavaScript argument limit', async () => {
    vi.mocked(invoke).mockResolvedValueOnce({ videos: Array(150000).fill(recordingFixtures[0]), warning: null })
      .mockResolvedValueOnce({ videos: [], warning: null });
    const result = await readVideoLibrary('192.168.43.8');
    expect(result.videos).toHaveLength(150000);
    expect(result.failed).toEqual([]);
  });
});
