import { useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { ArrowRight, Check, Copy, LoaderCircle, PlugZap, Search, ShieldCheck, Unplug, Wifi, X } from 'lucide-react';
import type { ConnectionDiagnostics, ConnectionProgress, NetworkInfo, Recorder } from './types';
import { connectRecorder, native } from './lib/recorder';

type Props = { busy: boolean; recorder: Recorder | null; demo: boolean; onClose: () => void; onDemo: () => void; onDisconnect: () => void; onConnect: (recorder: Recorder) => void };
const errorText = (error: unknown) => error instanceof Error ? error.message : String(error);

export default function ConnectDialog({ busy, recorder, demo, onClose, onDemo, onDisconnect, onConnect }: Props) {
  const dialog = useRef<HTMLDialogElement>(null);
  const generation = useRef(0);
  const [address, setAddress] = useState(() => localStorage.getItem('galaxy-address') || '');
  const [working, setWorking] = useState<'automatic' | 'manual' | 'check' | null>(null);
  const [network, setNetwork] = useState<NetworkInfo | null>(null);
  const [progress, setProgress] = useState('');
  const [error, setError] = useState('');
  const [diagnostics, setDiagnostics] = useState<ConnectionDiagnostics | null>(null);
  const [showDiagnostics, setShowDiagnostics] = useState(false);
  const [copyMessage, setCopyMessage] = useState('');

  useEffect(() => {
    dialog.current?.showModal();
    let closed = false;
    let unsubscribe: (() => void) | undefined;
    if (native) {
      invoke<NetworkInfo>('get_recorder_network').then(info => { if (!closed) setNetwork(info); }).catch(error => { if (!closed) setError(errorText(error)); });
      listen<ConnectionProgress>('connection-progress', ({ payload }) => { if (!closed) setProgress(payload.message); })
        .then(fn => { if (closed) fn(); else unsubscribe = fn; }).catch(() => {});
    }
    return () => {
      closed = true; generation.current++; unsubscribe?.();
      if (native) void invoke('cancel_recorder_discovery').catch(() => {});
    };
  }, []);

  async function readDiagnostics() {
    if (!native) { setError('请在手机或电脑应用中查看连接诊断'); return; }
    try { setDiagnostics(await invoke<ConnectionDiagnostics>('get_connection_diagnostics')); }
    catch (error) { setError(errorText(error)); }
  }

  async function run(method: 'automatic' | 'manual' | 'check') {
    const current = ++generation.current;
    setWorking(method); setError(''); setCopyMessage('');
    setProgress(method === 'automatic' ? '正在准备 Wi-Fi，并监听记录仪广播…' : method === 'check' ? '正在检查应用能否使用当前 Wi-Fi…' : '正在确认记录仪响应…');
    try {
      // Validate manual input before the native-only check so browser previews remain useful.
      if (method === 'manual') {
        const rec = await connectRecorder(address);
        if (current !== generation.current) return;
        localStorage.setItem('galaxy-address', rec.address); onConnect(rec);
      } else {
        if (!native) throw new Error('浏览器中可体验演示。真实连接请运行电脑版或手机版应用。');
        if (method === 'check') {
          const info = await invoke<NetworkInfo>('check_recorder_network');
          if (current !== generation.current) return;
          setNetwork(info);
          setProgress(info.platform === 'android' ? 'Wi-Fi 自检通过，应用可使用此网络。自检未查找记录仪。' : '局域网自检通过。自检未查找记录仪。');
        } else {
          const rec = await invoke<Recorder>('discover_recorder', { preferred: localStorage.getItem('galaxy-address') || null });
          if (current !== generation.current) return;
          localStorage.setItem('galaxy-address', rec.address); onConnect(rec);
        }
      }
    } catch (error) {
      if (current === generation.current) { setError(errorText(error)); setProgress(''); }
    } finally {
      if (current === generation.current) {
        setWorking(null);
        if (native) await readDiagnostics();
      }
    }
  }

  async function copyDiagnostics() {
    if (!diagnostics) return;
    try { await navigator.clipboard.writeText(JSON.stringify(diagnostics, null, 2)); setCopyMessage('已复制'); }
    catch { setCopyMessage('请长按下方文本选择并复制'); }
  }

  return <dialog ref={dialog} className="modal connect-dialog" aria-label="连接记录仪" onCancel={onClose} onClick={event => { if (event.target === event.currentTarget) onClose(); }}><div className="modal-content">
    <div className="modal-heading"><span className="modal-symbol"><Wifi size={23} /></span><button className="icon-button" aria-label="关闭连接窗口" onClick={onClose}><X size={20} /></button></div>
    <h2>连接你的星越 L</h2>
    <p className="modal-description">先在系统 Wi-Fi 中连接车机热点，再回到这里自动查找。</p>
    <div className="connection-steps"><div><span>1</span><div><strong>开启车机热点</strong><p>并打开车机上的原厂记录仪应用</p></div></div><div><span>2</span><div><strong>手机连接车机热点</strong><p>热点没有互联网也可以使用</p></div></div></div>
    <div className={'connection-network ' + (network?.available ? 'available' : '')} role="status"><Wifi size={18} /><div><strong>{network?.available ? '已检测到 Wi-Fi' : native ? '等待 Wi-Fi 连接' : '浏览器预览'}</strong><span>{network?.selected ? `${network.selected.ip} / ${network.selected.prefixLength} · 自动读取网络地址` : native ? '连接车机热点后，点击下方自动查找' : '在手机应用中可自动读取 Wi-Fi 并连接'}</span></div></div>
    {error && <div className="form-error" role="alert">{error}</div>}
    {busy && <div className="form-error">请先完成或取消传输，再切换设备。</div>}
    {progress && <p className="connection-progress" role="status">{working && <LoaderCircle className="spinning" size={15} />}{!working && !error && <Check size={15} />}{progress}</p>}
    <button className="primary-button full-width automatic-connect" disabled={Boolean(working) || busy} onClick={() => void run('automatic')}>{working === 'automatic' ? <LoaderCircle className="spinning" size={18} /> : <Search size={18} />}自动查找记录仪</button>
    {working === 'automatic' && <button className="quiet-button full-width" onClick={() => { setProgress('正在停止查找…'); void invoke('cancel_recorder_discovery').catch(error => setError(errorText(error))); }}>停止查找</button>}
    <div className="connection-tools"><button className="text-button" disabled={Boolean(working) || busy} onClick={() => void run('check')}><ShieldCheck size={15} />网络自检</button><button className="text-button" disabled={Boolean(working)} onClick={() => { setShowDiagnostics(!showDiagnostics); if (!showDiagnostics) void readDiagnostics(); }}>连接诊断</button></div>
    <details className="manual-connection" open={!native ? true : undefined}><summary>手动输入设备地址</summary><label className="form-field">记录仪地址<input placeholder="例如 192.168.43.123" value={address} disabled={Boolean(working) || busy} onChange={event => setAddress(event.target.value)} onKeyDown={event => { if (event.key === 'Enter' && !working && !busy) void run('manual'); }} /><small>原版显示的记录仪地址，可附加端口。</small></label><button className="secondary-button full-width" disabled={Boolean(working) || busy || !address.trim()} onClick={() => void run('manual')}><PlugZap size={16} />连接设备</button></details>
    {showDiagnostics && <section className="connection-diagnostics" aria-label="连接诊断"><div><strong>本次连接诊断</strong><button className="text-button" disabled={!diagnostics} onClick={() => void copyDiagnostics()}><Copy size={14} />{copyMessage || '复制诊断'}</button></div><p>记录网络状态与识别结果，便于排查连接问题。</p><textarea aria-label="诊断内容" readOnly value={diagnostics ? JSON.stringify(diagnostics, null, 2) : '尚无诊断信息，请先自动查找或运行网络自检。'} /></section>}
    {(recorder || demo) && <button className="quiet-button disconnect-action" disabled={busy || Boolean(working)} onClick={onDisconnect}><Unplug size={15} />断开当前连接</button>}
    <div className="modal-bottom"><span>现在不在车旁？</span><button className="text-button" disabled={busy || Boolean(working)} onClick={onDemo}>体验界面演示<ArrowRight size={14} /></button></div>
  </div></dialog>;
}
