import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Camera, Check, ChevronRight, HardDrive, LoaderCircle, Mic, RefreshCw, Settings2, ShieldAlert, Trash2, Wifi, X } from 'lucide-react';
import type { AppSettings, Recorder } from './types';
import { defaults, getSettings } from './lib/media';
import { native, formatSize } from './lib/recorder';
import Dialog from './Dialog';
import LogsDialog from './LogsDialog';
import {storageUsage,storageSummary,statusField,deviceVersions} from './lib/storage';

type Status = { fields: Record<string, string>; warnings: string[] };
type Props = { recorder: Recorder | null; savedCount: number; busy: boolean; onSaved: () => void; onConnect: () => void; onLive: () => void; onMessage: (text: string, error?: boolean) => void };
export default function Settings({ recorder, savedCount, busy, onSaved, onConnect, onLive, onMessage }: Props) {
  const [settings, setSettings] = useState<AppSettings>(defaults); const [status, setStatus] = useState<Status | null>(null); const [working, setWorking] = useState(''); const [statusError, setStatusError] = useState('');
  const [readingStatus, setReadingStatus] = useState(false);
  const [logsOpen, setLogsOpen] = useState(false);
  const [confirm, setConfirm] = useState(false); const [confirmText, setConfirmText] = useState(''); const [backedUp, setBackedUp] = useState(false); const [auth, setAuth] = useState('0');
  useEffect(() => { void getSettings().then(setSettings).catch(error => onMessage(String(error), true)); }, []);
  useEffect(() => { if (recorder) void refresh(); }, [recorder?.address]);
  async function refresh() {
    if (!recorder) return; setReadingStatus(true); setStatusError('');
    try { setStatus(await invoke<Status>('recorder_status', { address: recorder.address })); }
    catch (error) { setStatusError(String(error)); }
    finally { setReadingStatus(false); }
  }
  const field = (...names:string[])=>statusField(status?.fields??{},...names);
  const mic = field('mic', 'SoundRecord', 'MicSwitch');
  const recording = field('Record', 'Recording', 'Video','record');
  const usage = storageUsage(status?.fields ?? {});
  const card=storageSummary(status?.fields??{});const versions=deviceVersions(status?.fields??{},recorder?.firmware||'设备未提供');
  const storageSize = (bytes: number) => bytes === 0 ? '0 MB' : formatSize(bytes);
  async function save(next: AppSettings) {
    if (!native) { setSettings(next); return; }
    setWorking('settings');
    try { setSettings(await invoke<AppSettings>('save_settings', { settings: next })); onMessage('设置已保存'); }
    catch (error) { onMessage(String(error), true); }
    finally { setWorking(''); }
  }
  async function command(name: string, value: string | null = null) {
    if (name === 'format' && busy) { onMessage('请先完成或取消下载，再格式化存储卡', true); return; }
    if (!recorder) { onConnect(); return; } setWorking(name);
    try {
      await invoke('recorder_command', { address: recorder.address, command: name, value });
      onMessage(name === 'format' ? '设备已接受格式化指令，请等待记录仪完成' : '设备已接受操作，正在刷新状态');
      if (name === 'format') { setConfirm(false); setStatus(null); }
      await refresh();
    } catch (error) { onMessage(String(error), true); }
    finally { setWorking(''); }
  }
  async function changeMic(enabled: boolean) {
    if (!recorder) return; setWorking('mic');
    try {
      if (enabled&&recorder.profile!=='L6') await invoke('recorder_command', { address: recorder.address, command: 'authTime', value: auth });
      await command('mic', enabled ? 'ON' : 'OFF');
    } catch (error) { onMessage(String(error), true); setWorking(''); }
  }
  return <><div className="page-heading"><div><div className="eyebrow">MAKE IT YOURS</div><h1>设置与帮助<span className="heading-dot">.</span></h1><p>录像保存、设备状态与隐私导出。</p></div></div>
    <div className="settings-layout expanded-settings">
      <section className="surface-panel"><div className="section-header"><h2>下载与保存</h2><HardDrive size={18}/></div>
        <div className="setting-row"><strong>下载完成后保存到</strong><span className="fixed-save-location">系统相册</span></div>
        <div className="setting-row"><div><strong>已保存录像</strong><span>{savedCount} 段录像已保存</span></div><button className="text-button" onClick={onSaved}>查看<ChevronRight size={15} /></button></div>

      </section>
      <section className="surface-panel"><div className="section-header"><h2>播放器与隐私</h2><Settings2 size={18} /></div>
        <label className="setting-row"><div><strong>下载时遮挡车架号</strong><span>新任务下载后自动打码，再保存到系统相册</span></div><input aria-label="下载时遮挡车架号" type="checkbox" disabled={Boolean(working)} checked={Boolean(settings.maskDownloads)} onChange={event => void save({ ...settings, maskDownloads: event.target.checked })} /></label>
        <p className="setting-explanation">自动打码使用右上角默认水印区域，会重新编码。需要调整位置时，请在播放器中预览并导出。当前已排队任务沿用加入队列时的设置。</p>
        <label className="setting-row"><div><strong>导出时默认开启车架号打码</strong><span>在录像菜单中选择截取与打码后导出</span></div><input aria-label="默认开启车架号打码" type="checkbox" disabled={Boolean(working)} checked={settings.maskByDefault} onChange={event => void save({ ...settings, maskByDefault: event.target.checked })} /></label>
      </section>
      <section className="surface-panel device-settings"><div className="section-header"><h2>记录仪状态</h2><button className="text-button" disabled={!recorder || Boolean(working) || readingStatus} onClick={() => void refresh()}><RefreshCw className={readingStatus ? 'spinning' : ''} size={15} />刷新</button></div>
        {!recorder ? <div className="settings-connect"><Wifi size={28} /><p>连接记录仪后读取存储卡、固件和录音状态。</p><button className="primary-button" onClick={onConnect}>连接记录仪</button></div> : <>
          <div className="device-status-grid"><div><span>连接地址</span><strong>{recorder.address}</strong></div><div><span>录制状态</span><strong>{recording ? /^(0|stop|off)$/i.test(recording) ? '已暂停' : recording === '2' ? '正在录制紧急录像' : '正在录制普通录像' : '设备未提供'}</strong></div><div><span>固件版本</span><strong>{versions.firmware}</strong></div><div><span>控制器版本</span><strong>{versions.controller||'未读取到控制器版本'}</strong></div><div><span>存储卡总容量</span><strong>{card.total!=null?storageSize(card.total):'设备未提供'}</strong></div><div><span>可用空间</span><strong>{card.free!=null?storageSize(card.free):'设备未提供'}</strong></div></div>
          {usage ? <section className="storage-breakdown" aria-label="存储卡分类占用"><div className="storage-usage-bar" role="img" aria-label={`已用 ${Math.round(usage.used / usage.total * 100)}%`}>{usage.values.map((bytes, index) => <span key={index} className={'storage-segment segment-' + index} style={{ width: bytes / usage.total * 100 + '%' }} />)}</div><div className="storage-legend">{['普通录像', '紧急录像', '图片'].map((label, index) => <div key={label}><i className={'segment-' + index} /><span>{label}</span><strong>{storageSize(usage.values[index])} · {(usage.values[index] / usage.total * 100).toFixed(1)}%</strong></div>)}</div></section> : <p className="setting-explanation">{'存储卡状态：'+(card.state||'设备未提供')}</p>}
          <details className="status-details"><summary>查看设备返回的存储与状态字段</summary>{Object.entries(status?.fields ?? {}).map(([key, value]) => <div key={key}><span>{key}</span><code>{value}</code></div>)}{status?.warnings.length ? <p>部分字段未提供：{status.warnings.length} 项</p> : null}</details>
          {statusError && <p className="form-error" role="alert">{statusError}</p>}
          <div className="device-command-row"><button className="secondary-button" disabled={Boolean(working)||recorder.profile==='L6'} title={recorder.profile==='L6'?'此设备暂不支持实时预览':undefined} onClick={onLive}><Camera size={16} />实时画面</button><button className="secondary-button" disabled={Boolean(working)} onClick={() => void command('record')}>开始循环录制</button><button className="secondary-button" disabled={Boolean(working)} onClick={() => void command('stop')}>暂停录制</button><button className="secondary-button" disabled={Boolean(working)} onClick={() => void command('capture')}>拍照</button><button className="secondary-button" disabled={Boolean(working)} onClick={() => void command('event')}>保存紧急录像</button></div>
        </>}
      </section>
      <section className="surface-panel"><div className="section-header"><h2>录音与存储卡</h2><HardDrive size={18} /></div>
        <div className="setting-stack"><label className="setting-choice"><span><Mic size={15} />允许录制车内声音的时长</span><select aria-label="录音授权时长" value={auth} disabled={!recorder || Boolean(working)} onChange={event => setAuth(event.target.value)}><option value="0">360 天</option><option value="1">180 天</option><option value="2">90 天</option></select></label>
          <div className="setting-row"><div><strong>记录车内声音</strong><span>当前：{mic ? /^(ON|1|true)$/i.test(mic) ? '已开启' : '已关闭' : '设备未提供状态'}</span></div><button className="secondary-button" disabled={!recorder || Boolean(working)} onClick={() => void changeMic(!/^(ON|1|true)$/i.test(mic || ''))}>{/^(ON|1|true)$/i.test(mic || '') ? '关闭录音' : '开启录音'}</button></div>
          <div className="setting-row danger-setting"><div><strong>格式化记录仪存储卡</strong><span>{busy ? '请先完成或取消下载' : '清空记录仪内的全部录像和照片'}</span></div><button className="text-button danger-text" disabled={!recorder || Boolean(working) || busy} onClick={() => { setConfirmText(''); setBackedUp(false); setConfirm(true); }}><Trash2 size={15} />格式化</button></div>
        </div>
      </section>
      <section className="surface-panel"><div className="section-header"><h2>连接帮助与诊断</h2><Wifi size={18} /></div><p className="setting-explanation">手机连接车机热点，并在车机打开原厂记录仪。热点无互联网时仍可使用。录像或预览读取异常，可导出运行日志协助排查。</p><button className="secondary-button" onClick={onConnect}>管理连接<ChevronRight size={15} /></button><button className="secondary-button" onClick={() => setLogsOpen(true)}>查看运行日志</button><p className="setting-explanation">吉行影记 v0.8.5<br />连接方案：A7 / E5 / L6 与通用模式</p></section>
    </div>
    {logsOpen && <LogsDialog onClose={() => setLogsOpen(false)} onMessage={onMessage} />}
    {confirm && <Dialog label="确认格式化存储卡" onClose={() => { if (working !== 'format') setConfirm(false); }}><div className="modal-content"><div className="modal-heading"><ShieldAlert size={28} /><button className="icon-button" aria-label="取消格式化" disabled={working === 'format'} onClick={() => setConfirm(false)}><X size={20} /></button></div><h2>清空记录仪存储卡？</h2><p className="modal-description">记录仪内所有录像和照片都会被删除，无法撤销。已保存到相册或文件夹的录像不受影响。</p><label className="toggle-row"><span>我已备份需要保留的录像</span><input aria-label="确认已备份" type="checkbox" checked={backedUp} onChange={event => setBackedUp(event.target.checked)} /></label><label className="form-field">输入“清空”以确认<input aria-label="格式化确认文字" value={confirmText} onChange={event => setConfirmText(event.target.value)} /></label><button className="primary-button danger-button full-width" disabled={!backedUp || confirmText !== '清空' || Boolean(working) || busy} onClick={() => void command('format', '确认清空存储卡')}>{working === 'format' ? <LoaderCircle className="spinning" size={16} /> : <Check size={16} />}确认格式化</button></div></Dialog>}
  </>;
}
