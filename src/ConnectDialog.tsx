import { useState } from 'react';
import { LoaderCircle, QrCode, ChevronRight, Unplug, Wifi, X } from 'lucide-react';
import type { Recorder,ConnectionProfile,ConnectOptions } from './types';
import {connectionProfiles} from './lib/vehicle';
import {validateAddress} from './lib/recorder';
import Dialog from './Dialog';
import SchemePicker from './SchemePicker';
import { android, ios, nativeMedia, parseWifiQr } from './lib/media';

type Props = {profile:ConnectionProfile;onProfileChange:(profile:ConnectionProfile)=>Promise<void>; busy: boolean; connecting: boolean; recorder: Recorder | null; onClose: () => void; onDisconnect: () => void; onConnect: (options?:ConnectOptions) => Promise<Recorder>; onCancel: () => void };
export default function ConnectDialog({ profile,onProfileChange,busy, connecting, recorder, onClose, onDisconnect, onConnect, onCancel }: Props) {
  const [schemeOpen,setSchemeOpen]=useState(false);
  const [scanning, setScanning] = useState(false); const [error, setError] = useState('');
  const [password,setPassword]=useState('12345678');const [openWifi,setOpenWifi]=useState(false);const [manual,setManual]=useState('');const [changing,setChanging]=useState(false);
  const working = connecting || scanning || changing;
  async function connect(options:ConnectOptions={}) { setError(''); try { await onConnect(options); onClose(); } catch { setError('连接未成功，请确认已连接车机热点，并开启车机记录仪后再试。'); } }
  async function scan(image: boolean) {
    setScanning(true); setError('');
    try {
      const result = await nativeMedia<{ text: string | null }>(image ? 'scanImage' : 'scan');
      if (!result.text) return;
      await nativeMedia('connectWifi', parseWifiQr(result.text));
      await onConnect({force:true}); onClose();
    } catch (error) { setError(String(error).includes('二维码') ? String(error) : '连接未完成，请确认热点已开启，并允许系统连接后重试。'); }
    finally { setScanning(false); }
  }
  async function search(){setScanning(true);setError('');try{await nativeMedia('searchRecorderWifi',{password:openWifi?'':password});await onConnect({force:true,profile:profile==='AUTO'?'E5':profile});onClose();}catch(error){setError(String(error));}finally{setScanning(false);}}
  async function change(profile:ConnectionProfile){setChanging(true);setError('');try{await onProfileChange(profile);setSchemeOpen(false);}catch(error){setError(String(error));throw error;}finally{setChanging(false);}}
  return <Dialog label="连接记录仪" className="connect-dialog" onClose={onClose}><div className="modal-content">
    <div className="modal-heading"><span className="modal-symbol"><Wifi size={23} /></span><button className="icon-button" aria-label="关闭连接窗口" onClick={onClose}><X size={20} /></button></div>
    <h2>{recorder ? '记录仪已连接' : '连接你的记录仪'}</h2>
    <p className="modal-description">{ios ? '开启车机热点与记录仪，先在 iPhone 的系统设置中连接车机 Wi-Fi，再回到这里点「连接」。首次连接请允许访问本地网络。' : '开启车机热点与记录仪，连接热点后点「连接」，或扫描车机热点二维码。'}</p>
    {error && <div className="form-error" role="alert">{error}</div>}
    {busy && <p className="form-error">传输中，请先暂停下载再切换连接。</p>}
    {working && <p className="connection-progress" role="status"><LoaderCircle className="spinning" size={15} />正在连接，请稍候…</p>}
    <button className="connection-scheme-trigger" aria-label="选择连接方案" disabled={working||busy} onClick={()=>setSchemeOpen(true)}><span><small>连接方案</small><strong>{connectionProfiles.find(option=>option.value===profile)?.label}</strong><span>{connectionProfiles.find(option=>option.value===profile)?.description}</span></span><ChevronRight size={20}/></button>
    {!recorder && <button className="primary-button full-width automatic-connect" disabled={working || busy} onClick={() => void connect({force:true})}><Wifi size={18} />连接记录仪</button>}
    {connecting && <button className="quiet-button full-width" onClick={onCancel}>取消连接</button>}
    {android && !recorder && <div className="wifi-scan-actions"><button className="secondary-button" disabled={working || busy} onClick={() => void scan(false)}><QrCode size={17} />扫码连接</button><button className="secondary-button" disabled={working || busy} onClick={() => void scan(true)}>从相册识别</button><button className="text-button" onClick={() => void nativeMedia('openWifi')}>打开 Wi-Fi 设置</button></div>}
    {recorder&&<button className="primary-button full-width" disabled={working||busy} onClick={()=>void connect({force:true})}>按所选方案重新连接</button>}
    {android&&!recorder&&<details className="connection-alternatives"><summary>搜索记录仪 Wi-Fi</summary><p className="setting-explanation">适用于原厂 GEELY_ 记录仪热点。连接时请在系统弹窗中确认。</p><label className="form-field">记录仪 Wi-Fi 密码<input aria-label="记录仪 Wi-Fi 密码" type="password" value={password} disabled={working||openWifi} onChange={event=>setPassword(event.target.value)}/></label><label className="toggle-row"><span>记录仪热点没有密码</span><input aria-label="无密码热点" type="checkbox" checked={openWifi} disabled={working} onChange={event=>setOpenWifi(event.target.checked)}/></label><button className="secondary-button full-width" disabled={working||busy} onClick={()=>void search()}>搜索并连接记录仪</button></details>}
    {!recorder&&<details className="connection-alternatives"><summary>手动连接已知设备</summary><p className="setting-explanation">自动连接未成功时，可填写原 APP 中的记录仪地址。</p><label className="form-field">记录仪地址<input aria-label="记录仪地址" placeholder="例如 192.168.43.123" value={manual} disabled={working} onChange={event=>setManual(event.target.value)}/></label><button className="secondary-button full-width" disabled={working||busy||!manual.trim()} onClick={()=>{try{void connect({force:true,address:validateAddress(manual)})}catch(error){setError(String(error));}}}>连接此设备</button></details>}
    {recorder && <button className="quiet-button disconnect-action" disabled={busy || working} onClick={onDisconnect}><Unplug size={15} />断开连接</button>}
    <p className="setting-explanation">每次打开会自动尝试连接；在首页下拉也可重新连接。已连接时可直接使用。</p>
    {schemeOpen&&<SchemePicker profile={profile} busy={changing} onClose={()=>{if(!changing)setSchemeOpen(false);}} onConfirm={change}/>}
  </div></Dialog>;
}
