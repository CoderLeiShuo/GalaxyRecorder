import { useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Download, FolderOpen, LoaderCircle, Share2, X } from 'lucide-react';
import type { AppSettings, Video } from './types';
import { mobile, getSettings } from './lib/media';
import Dialog from './Dialog';

export default function BatchActions({ videos, onClose, onSaved, onMessage }: { videos: Video[]; onClose: () => void; onSaved: (video: Video) => void; onMessage: (text: string, error?: boolean) => void }) {
  const [working, setWorking] = useState(false); const [done, setDone] = useState(0); const [errors, setErrors] = useState<string[]>([]); const [summary, setSummary] = useState('');
  const cancelled = useRef(false); const real = videos.filter(video => video.savedPath);
  async function share(original: boolean) {
    setWorking(true);
    try { await invoke('share_saved_many', { ids: real.map(video => video.id), original }); onMessage(mobile ? '已打开系统分享' : '已打开录像位置'); }
    catch (error) { onMessage(String(error), true); }
    finally { setWorking(false); }
  }
  async function exportAll(mode: 'gallery' | 'folder') {
    setWorking(true); setDone(0); setErrors([]); setSummary(''); cancelled.current = false;
    try {
      if (mode === 'folder') {
        const settings = await getSettings();
        if (!settings.directory) {const selected=await invoke<AppSettings|null>('choose_download_directory');if(!selected)return;if(settings.saveMode!=='folder')await invoke('save_settings',{settings:{...selected,saveMode:settings.saveMode}});}
      }
      let success = 0; const failures: string[] = [];
      for (const video of real) {
        if (cancelled.current) break;
        try { onSaved(await invoke<Video>('export_saved', { id: video.id, mode })); success++; }
        catch (error) { failures.push((video.time || video.name) + '：' + String(error)); setErrors([...failures]); }
        setDone(value => value + 1);
      }
      setSummary(`已导出 ${success} 段${failures.length ? `，失败 ${failures.length} 段` : ''}${cancelled.current ? '，已停止后续导出' : ''}`);
    } catch (error) { onMessage(String(error), true); }
    finally { setWorking(false); }
  }
  return <Dialog label="批量导出与分享" onClose={() => { if (!working) onClose(); }}><div className="modal-content">
    <div className="modal-heading"><h2>导出与分享</h2><button className="icon-button" aria-label="关闭批量操作" disabled={working} onClick={onClose}><X size={20} /></button></div>
    <p className="modal-description">已选择 {real.length} 段已下载录像。分享与导出操作使用实际保存的录像文件。</p>
    <div className="batch-action-grid"><button className="primary-button" disabled={working || !real.length} onClick={() => void exportAll('gallery')}><Download size={16} />{mobile ? '批量保存到相册' : '导出到视频文件夹'}</button>
      <button className="secondary-button" disabled={working || !real.length} onClick={() => void exportAll('folder')}><FolderOpen size={16} />导出到指定目录</button>
      <button className="secondary-button" disabled={working || !real.length} onClick={() => void share(false)}><Share2 size={16} />{mobile ? '批量分享视频' : '打开录像位置'}</button>
      {mobile && <button className="secondary-button" disabled={working || !real.length} onClick={() => void share(true)}><Share2 size={16} />以原文件分享</button>}</div>
    {mobile && <p className="setting-explanation">原文件分享以文件类型交给接收应用，本应用不重新编码。接收应用是否压缩，取决于所选的发送方式。</p>}
    {working && <div className="export-status"><LoaderCircle className="spinning" size={16} />正在处理 {done} / {real.length}<button className="text-button" onClick={() => { cancelled.current = true; setSummary('将在当前文件完成后停止'); }}>停止后续导出</button></div>}
    {summary && <p role="status" className="setting-explanation">{summary}</p>}{errors.length > 0 && <div role="alert" className="batch-errors">{errors.map((error, index) => <p key={index}>{error}</p>)}</div>}
  </div></Dialog>;
}
