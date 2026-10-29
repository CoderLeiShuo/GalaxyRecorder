import { invoke } from '@tauri-apps/api/core';
import type { AppSettings, Video, VideoMetadata } from '../types';
import { native } from './recorder';
export const android = native && /Android/i.test(navigator.userAgent);
export const ios = native && (/iPhone|iPad|iPod/i.test(navigator.userAgent) || (navigator.platform === 'MacIntel' && navigator.maxTouchPoints > 1));
export const mobile = android || ios;
export const defaults: AppSettings = { saveMode: 'gallery', directory: null, directoryLabel: null, maskByDefault: false, maskDownloads: false, keepScreenOn: true };
export const nativeMedia = <T = Record<string, unknown>>(operation: string, payload: Record<string, unknown> = {}) => invoke<T>('native_media', { operation, payload });
export const getSettings = () => native ? invoke<AppSettings>('get_settings') : Promise.resolve(defaults);

const cache = new Map<string, Promise<VideoMetadata>>();
const pending: (() => Promise<void>)[] = [];
let active = 0;
function pump() { while (active < 2 && pending.length) { active++; void pending.shift()!().finally(() => { active--; pump(); }); } }
export function clearMetadataCache() { cache.clear(); }
export function getMetadata(address: string | undefined, video: Video): Promise<VideoMetadata> {
  const key = [address, video.savedPath, video.id, video.size].join('|');
  let result = cache.get(key);
  if (!result) {
    result = new Promise<VideoMetadata>((resolve, reject) => {
      pending.push(async () => {
        try { resolve(await invoke<VideoMetadata>('video_metadata', { address: address ?? null, video })); }
        catch (error) { cache.delete(key); reject(error); }
      }); pump();
    }); cache.set(key, result);
  }
  return result;
}
export function clampSeek(value: number, duration: number) { return Math.max(0, Math.min(Number.isFinite(value) ? value : 0, Math.max(0, duration))); }
export function playerTime(value: number) {
  const seconds = Math.floor(Math.max(0, Number.isFinite(value) ? value : 0));
  return (seconds >= 3600 ? Math.floor(seconds / 3600) + ':' : '') + Math.floor(seconds / 60 % 60).toString().padStart(2, '0') + ':' + (seconds % 60).toString().padStart(2, '0');
}
export function parseWifiQr(text: string): { ssid: string; password: string } {
  if (!text.startsWith('WIFI:')) throw new Error('这不是 Wi-Fi 热点二维码');
  const fields: Record<string, string> = {}; let token = ''; let escaped = false;
  for (const char of text.slice(5) + ';') {
    if (escaped) { token += char; escaped = false; }
    else if (char === '\\') escaped = true;
    else if (char === ';') { const i = token.indexOf(':'); if (i > 0) fields[token.slice(0, i)] = token.slice(i + 1); token = ''; }
    else token += char;
  }
  if (!fields.S) throw new Error('二维码中没有热点名称');
  if (fields.T && !['WPA', 'WPA2', 'nopass'].includes(fields.T)) throw new Error('请在系统设置中连接此加密类型的热点');
  return { ssid: fields.S, password: fields.P ?? '' };
}
