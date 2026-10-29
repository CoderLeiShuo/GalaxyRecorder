import { isTauri, invoke, convertFileSrc } from '@tauri-apps/api/core';
import type { Recorder, Video, Category } from '../types';

export const native = isTauri();

export function validateAddress(input: string): string {
  const text = input.trim().replace(/^http:\/\//i, '').replace(/\/$/, '');
  const match = /^(\d{1,3}(?:\.\d{1,3}){3})(?::(\d{1,5}))?$/.exec(text);
  if (!match) throw new Error('请输入记录仪的局域网 IP，可附加端口');
  const octets = match[1].split('.').map(Number);
  if (octets.some((n, i) => n > 255 || String(n) !== match[1].split('.')[i])) throw new Error('IP 地址格式不正确');
  const [a, b] = octets;
  if (!(['193.168.0.1','192.72.1.1'].includes(match[1]) || a === 10 || (a === 172 && b >= 16 && b <= 31) || (a === 192 && b === 168))) throw new Error('请使用车机热点内的局域网地址');
  const port = Number(match[2] || 80);
  if (port < 1 || port > 65535) throw new Error('端口必须在 1–65535 之间');
  return match[1] + (port === 80 ? '' : ':' + port);
}

export async function connectRecorder(address: string): Promise<Recorder> {
  const valid = validateAddress(address);
  if (!native) throw new Error('真实设备连接请在桌面或手机应用中使用');
  return invoke('probe_recorder', { address: valid });
}
export async function readVideos(address: string, category: Category): Promise<{videos:Video[];warning:string|null}> {
  return invoke('list_videos', { address, category });
}
export async function readVideoLibrary(address: string, isCurrent = () => true): Promise<{videos: Video[]; warnings: string[]; failed: Category[]}> {
  const videos: Video[] = [];
  const warnings: string[] = [];
  const failed: Category[] = [];
  // Follow the original application's order; some devices only handle one listing at a time.
  for (const category of ['normal', 'event'] as const) {
    if (!isCurrent()) break;
    const label = category === 'normal' ? '普通录像' : '紧急录像';
    try {
      const result = await readVideos(address, category);
      for (const video of result.videos) videos.push(video);
      if (result.warning) warnings.push(label + '：' + result.warning);
    } catch (error) {
      failed.push(category);
      warnings.push(label + '：' + (error instanceof Error ? error.message : String(error)));
    }
  }
  return { videos, warnings, failed };
}
export function downloadVideo(address: string, video: Video, taskId: string): Promise<Video> {
  return invoke('download_video', { address, video, taskId });
}
export function cancelDownload(taskId: string): Promise<void> { return invoke('cancel_download', { taskId }); }
export function readSaved(): Promise<Video[]> { return native ? invoke('list_saved') : Promise.resolve([]); }
export async function revealSaved(): Promise<void> {
  if (!native) return;
  const folder = await invoke<string>('saved_directory');
  const { openPath } = await import('@tauri-apps/plugin-opener');
  await openPath(folder);
}
export function videoSource(video: Video): string | undefined { return native && video.savedPath ? convertFileSrc(video.savedPath) : undefined; }
export function formatSize(bytes: number | null): string {
  if (bytes === null || bytes <= 0) return '大小待确认';
  return bytes >= 1024 ** 3 ? (bytes / 1024 ** 3).toFixed(1) + ' GB' : (bytes / 1024 ** 2).toFixed(1) + ' MB';
}
export function formatDuration(seconds: number | null): string {
  if (seconds === null) return '时长待读取';
  return Math.floor(seconds / 60).toString().padStart(2, '0') + ':' + Math.floor(seconds % 60).toString().padStart(2, '0');
}

export function visibleVideos(videos: Video[], filter: string, date: string, query: string): Video[] {
  return videos.filter(v => (filter === 'all' || (filter === 'saved' ? Boolean(v.savedPath) : v.category === filter)) && (!date || v.date === date) && (!query || (v.name + v.date + v.time).includes(query))).sort((a,b)=>(b.date+b.time).localeCompare(a.date+a.time));
}

// Preserve the filtered, newest-first order while paginating across date groups.
export function groupVideosByDate(videos: Video[], limit: number) {
  const groups = new Map<string, { date: string; videos: Video[]; count: number }>();
  videos.forEach((video,index) => {
    let group=groups.get(video.date);
    if(!group){group={date:video.date,videos:[],count:0};groups.set(video.date,group);}
    group.count++;
    if(index<limit)group.videos.push(video);
  });
  return [...groups.values()].filter(group=>group.videos.length>0);
}
