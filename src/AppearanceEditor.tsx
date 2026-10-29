import {useRef,useState} from 'react';
import {ImagePlus,LoaderCircle,Pencil,RotateCcw,X} from 'lucide-react';
import {invoke} from '@tauri-apps/api/core';
import Dialog from './Dialog';
import {native} from './lib/recorder';
import {appearanceFromSettings,defaultDeviceTitle,prepareDeviceImage} from './lib/appearance';
import type {Appearance,AppSettings,DeviceMemory} from './types';
export default function AppearanceEditor({kind,appearance,device,onClose,onSaved,onVehicleDetails,onAppName}:{kind:'title'|'vehicle'|'image';appearance:Appearance;device:DeviceMemory|null;onClose:()=>void;onSaved:(value:Appearance)=>void;onVehicleDetails:()=>void;onAppName:()=>void}){
 const title=kind==='title'?'修改记录仪标题':kind==='vehicle'?'自定义车辆名称':'更换记录仪图片';
 const [draft,setDraft]=useState(kind==='title'?appearance.deviceTitle:device&&appearance.vehicleAliases[device.key]||device?.vehicle?.name||'我的车辆');
 const [working,setWorking]=useState(false);const [error,setError]=useState('');const input=useRef<HTMLInputElement>(null);
 async function save(value:string|null){
  setWorking(true);setError('');
  try{
   let next:Appearance;
   if(native)next=appearanceFromSettings(await invoke<AppSettings>('save_appearance',{kind,value,deviceKey:device?.key||null}));
   else{
    next={...appearance,vehicleAliases:{...appearance.vehicleAliases}};
    if(kind==='title')next.deviceTitle=value||defaultDeviceTitle;
    if(kind==='image')next.deviceImage=value;
    if(kind==='vehicle'&&device){if(value)next.vehicleAliases[device.key]=value;else delete next.vehicleAliases[device.key];}
   }
   // The native settings file owns persistence. Cache text only to keep startup light.
   try{localStorage.setItem('galaxy-appearance',JSON.stringify(native?{...next,deviceImage:null}:next));}catch(error){if(!native)throw error;}
   onSaved(next);onClose();
  }catch(error){setError(String(error));}finally{setWorking(false);}
 }
 async function choose(file:File|undefined){if(!file)return;setWorking(true);setError('');try{await save(await prepareDeviceImage(file));}catch(error){setError(String(error));setWorking(false);}}
 return <Dialog label={title} className="appearance-dialog" onClose={()=>{if(!working)onClose();}}><div className="modal-content">
  <div className="modal-heading"><span className="modal-symbol">{kind==='image'?<ImagePlus size={23}/>:<Pencil size={22}/>}</span><button className="icon-button" aria-label="关闭外观设置" disabled={working} onClick={onClose}><X size={20}/></button></div><h2>{title}</h2>
  {kind==='image'?<><p className="modal-description">选择透明背景的 PNG 或 WebP 图片，保留原图比例。保存后，下次打开仍会使用这张图片。</p><div className="appearance-image-preview"><img src={appearance.deviceImage||'/images/glacier-device.webp'} alt="当前记录仪图片"/></div><input ref={input} hidden type="file" accept="image/png,image/webp" aria-label="选择记录仪图片" onChange={event=>{void choose(event.target.files?.[0]);event.target.value='';}}/><button className="primary-button full-width" disabled={working} onClick={()=>input.current?.click()}><ImagePlus size={18}/>选择图片</button><button className="quiet-button full-width" disabled={working||!appearance.deviceImage} onClick={()=>void save(null)}><RotateCcw size={16}/>恢复默认图片</button></>:<form onSubmit={event=>{event.preventDefault();void save(draft.trim());}}>
   <p className="modal-description">{kind==='title'?'默认将车辆名称与软件名称合成标题，也可以起一个专属标题。':`原车型：${device?.vehicle?.name||'尚未识别'}。自定义名称不会被重新识别的车型覆盖。`}</p>
   <label className="form-field">{kind==='title'?'记录仪标题':'车辆显示名称'}<input autoFocus aria-label={kind==='title'?'记录仪标题':'车辆显示名称'} maxLength={32} value={draft} disabled={working} onChange={event=>setDraft(event.target.value)}/></label>
   {kind==='title'&&<button type="button" className="text-button appearance-details" disabled={working} onClick={onAppName}>修改软件名称</button>}
   {kind==='vehicle'&&<button type="button" className="text-button appearance-details" disabled={working} onClick={onVehicleDetails}>查看或补充车型与配置</button>}
   <div className="name-editor-actions"><button type="button" className="text-button" disabled={working} onClick={()=>void save(null)}><RotateCcw size={14}/>{kind==='title'?'恢复默认标题':'恢复车型名称'}</button><button type="button" className="secondary-button" disabled={working} onClick={onClose}>取消</button><button className="primary-button" disabled={working||!draft.trim()}>保存</button></div>
  </form>}
  {working&&<p className="connection-progress" role="status"><LoaderCircle size={16} className="spinning"/>正在保存…</p>}{error&&<p className="form-error" role="alert">{error}</p>}
 </div></Dialog>;
}
