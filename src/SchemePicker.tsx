import {useState} from 'react';
import {CarFront,Check,Circle,LoaderCircle,Settings2,X} from 'lucide-react';
import Dialog from './Dialog';
import {connectionProfiles} from './lib/vehicle';
import type {ConnectionProfile} from './types';
export default function SchemePicker({profile,busy,onClose,onConfirm}:{profile:ConnectionProfile;busy:boolean;onClose:()=>void;onConfirm:(value:ConnectionProfile)=>Promise<void>}){
 const [selected,setSelected]=useState(profile);const [error,setError]=useState('');
 async function confirm(){try{await onConfirm(selected);}catch(error){setError(String(error));}}
 return <Dialog label="连接方案" className="scheme-picker" onClose={onClose}>
  <div className="sheet-handle"/><header className="scheme-heading"><div><h2>连接方案</h2><p>不知道选哪个？选择自动连接</p></div><button className="icon-button" disabled={busy} aria-label="关闭方案选择" onClick={onClose}><X size={21}/></button></header>
  <div className="scheme-list" role="radiogroup" aria-label="连接方案列表">
   {connectionProfiles.slice(0,4).map(option=><button key={option.value} type="button" role="radio" aria-checked={selected===option.value} aria-label={option.label} disabled={busy} className={'scheme-card '+(selected===option.value?'selected':'')} onClick={()=>setSelected(option.value)}>
    <span className="scheme-card-heading">{option.value==='AUTO'?<Settings2 size={22}/>:<CarFront size={22}/>}<strong>{option.label}</strong>{option.value==='AUTO'&&<em>推荐</em>}<span className="scheme-radio">{selected===option.value?<Check size={13}/>:<Circle size={21}/>}</span></span>
    {option.models.length>0&&<span className="scheme-models">{option.models.map(model=><span key={model}>{model}</span>)}</span>}
    <span className="scheme-description">{option.description}</span>
   </button>)}
   <p className="scheme-generic-hint">未列出的车型，可尝试以下通用模式</p><div className="scheme-generics">{connectionProfiles.slice(4).map(option=><button role="radio" type="button" aria-label={option.label} aria-checked={selected===option.value} disabled={busy} key={option.value} className={'scheme-card '+(selected===option.value?'selected':'')} onClick={()=>setSelected(option.value)}><strong>{option.label}</strong><span>{option.description}</span></button>)}</div>
  </div>
  <footer className="scheme-footer"><p><Check size={15}/>已选方案会保存在本机，下次自动使用</p>{error&&<p className="form-error" role="alert">{error}</p>}<button className="primary-button full-width" disabled={busy} onClick={()=>void confirm()}>{busy&&<LoaderCircle size={17} className="spinning"/>}使用此方案</button></footer>
 </Dialog>;
}
