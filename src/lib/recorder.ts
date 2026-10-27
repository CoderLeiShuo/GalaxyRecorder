import { isTauri, invoke, convertFileSrc } from '@tauri-apps/api/core';
import type { Recorder, Video, Category } from '../types';

export const native = isTauri();
export const targetVehicle = '2026 款星越 L';

export function validateAddress(input: string): string {
  const text = input.trim().replace(/^http:\/\//i, '').replace(/\/$/, '');
  const match = /^(\d{1,3}(?:\.\d{1,3}){3})(?::(\d{1,5}))?$/.exec(text);
  if (!match) throw new Error('请输入记录仪的局域网 IP，可附加端口');
  const octets = match[1].split('.').map(Number);
  if (octets.some((n, i) => n > 255 || String(n) !== match[1].split('.')[i])) throw new Error('IP 地址格式不正确');
  const [a, b] = octets;
  if (!(a === 10 || (a === 172 && b >= 16 && b <= 31) || (a === 192 && b === 168))) throw new Error('请使用车机热点内的局域网地址');
  const port = Number(match[2] || 80);
  if (port < 1 || port > 65535) throw new Error('端口必须在 1–65535 之间');
  return match[1] + (port === 80 ? '' : ':' + port);
}

export async function connectRecorder(address: string): Promise<Recorder> {
  const valid = validateAddress(address);
  if (!native) throw new Error('真实设备连接请在桌面或手机应用中使用，浏览器可体验演示');
  return invoke('probe_recorder', { address: valid });
}
export async function readVideos(address: string, category: Category): Promise<{videos:Video[];warning:string|null}> {
  return invoke('list_videos', { address, category });
}
export function downloadVideo(address: string, video: Video, taskId: string): Promise<string> {
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
export function videoSource(video: Video): string | undefined { return native && !video.demo && video.savedPath ? convertFileSrc(video.savedPath) : undefined; }
export function formatSize(bytes: number | null): string {
  if (bytes === null || bytes <= 0) return '大小待确认';
  return bytes >= 1024 ** 3 ? (bytes / 1024 ** 3).toFixed(1) + ' GB' : (bytes / 1024 ** 2).toFixed(1) + ' MB';
}
export function formatDuration(seconds: number | null): string {
  if (seconds === null) return '时长待读取';
  return Math.floor(seconds / 60).toString().padStart(2, '0') + ':' + (seconds % 60).toString().padStart(2, '0');
}

export function visibleVideos(videos: Video[], filter: string, date: string, query: string): Video[] {
  return videos.filter(v => (filter === 'all' || (filter === 'saved' ? Boolean(v.savedPath) : v.category === filter)) && (!date || v.date === date) && (!query || (v.name + v.date + v.time).includes(query))).sort((a,b)=>(b.date+b.time).localeCompare(a.date+a.time));
}

export const demoVideos: Video[] = [
  ['08:26:40', 'normal', 186], ['08:23:40', 'normal', 184], ['08:21:06', 'event', 32],
  ['08:20:40', 'normal', 185], ['08:17:40', 'normal', 182], ['08:14:40', 'normal', 188],
  ['08:12:18', 'event', 34], ['08:11:40', 'normal', 183],
].map(([time, category, size], i)=>({id:'demo-'+i,name:'20261027_'+String(time).replaceAll(':','')+'.mp4',path:'/demo/'+i+'.mp4',category:category as Category,date:'2026-10-27',time:String(time),size:Number(size)*1024**2,duration:category==='event'?30:180,demo:true}));
