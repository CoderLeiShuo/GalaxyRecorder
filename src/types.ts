export type Page = 'device' | 'videos' | 'transfers' | 'settings';
export type Category = 'normal' | 'event';
export type Filter = 'all' | Category | 'saved';
export type Video = {
  id: string;
  name: string;
  path: string;
  category: Category;
  date: string;
  time: string;
  size: number;
  duration: number | null;
  savedPath?: string;
  demo?: boolean;
};
export type Recorder = { address: string; protocol: string; firmware: string; vehicle: string };
export type Task = {
  id: string;
  video: Video;
  status: 'waiting' | 'downloading' | 'done' | 'cancelled' | 'failed';
  received: number;
  total: number | null;
  error?: string;
  savedPath?: string;
};
export type DownloadEvent = { taskId: string; received: number; total: number | null };
