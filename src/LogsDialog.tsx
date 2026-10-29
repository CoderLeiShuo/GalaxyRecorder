import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Copy, Download, RefreshCw, Share2, X } from 'lucide-react';
import Dialog from './Dialog';
import { mobile } from './lib/media';
import { native } from './lib/recorder';

export default function LogsDialog({ onClose, onMessage }: { onClose: () => void; onMessage: (text: string, error?: boolean) => void }) {
  const [text, setText] = useState(''); const [working, setWorking] = useState(false);
  async function refresh() {
    setWorking(true);
    try { setText(native ? await invoke<string>('get_operation_logs') : '界面预览：运行日志会在手机或电脑应用内记录。'); }
    catch (error) { onMessage(String(error), true); }
    finally { setWorking(false); }
  }
  useEffect(() => { void refresh(); }, []);
  async function exportLogs(share: boolean) {
    setWorking(true);
    try { if (await invoke<boolean>('export_operation_logs', { share })) onMessage(share && mobile ? '已打开日志分享' : '日志文件已保存'); }
    catch (error) { onMessage(String(error), true); }
    finally { setWorking(false); }
  }
  return <Dialog label="运行日志" onClose={() => { if (!working) onClose(); }}><div className="modal-content operation-logs">
    <div className="modal-heading"><h2>运行日志</h2><button className="icon-button" aria-label="关闭运行日志" disabled={working} onClick={onClose}><X size={20} /></button></div>
    <p className="setting-explanation">记录最近的连接、下载、播放与导出操作，退出后仍可查看。日志保存在本机，分享和导出由你主动操作。</p>
    <textarea aria-label="运行日志内容" readOnly value={text || '正在读取日志…'} />
    <div className="batch-action-grid"><button className="secondary-button" disabled={working || !text} onClick={() => void navigator.clipboard.writeText(text).then(() => onMessage('日志已复制')).catch(error => onMessage(String(error), true))}><Copy size={15} />复制日志</button>
      <button className="secondary-button" disabled={working} onClick={() => void refresh()}><RefreshCw size={15} />刷新日志</button>
      <button className="secondary-button" disabled={working || !text || !native} onClick={() => void exportLogs(false)}><Download size={15} />导出日志文件</button>
      {mobile && <button className="primary-button" disabled={working || !text} onClick={() => void exportLogs(true)}><Share2 size={15} />分享日志</button>}</div>
  </div></Dialog>;
}
