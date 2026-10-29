import { useRef, useState } from 'react';
import { RefreshCw, Camera, ArrowDownUp, ChevronRight, Clapperboard, Settings, Video, Wifi, Link2 } from 'lucide-react';
import type { Page, Recorder,VehicleInfo } from './types';

type Props = {
  title:string;image:string|null;onTitle:()=>void;onImage:()=>void;
  vehicleName:string;vehicleInfo:VehicleInfo|null;onVehicle:()=>void;
  recorder: Recorder | null;
  checking: boolean;
  connecting: boolean;
  onReconnect: () => Promise<void>;
  busy: boolean;
  videoCount: number;
  savedCount: number;
  pendingCount: number;
  onConnect: () => void;
  onLive: () => void;
  onNavigate: (page: Page) => void;
};

export default function DeviceHome({ title,image,onTitle,onImage,vehicleName,vehicleInfo,onVehicle,recorder, checking, connecting, onReconnect, busy, videoCount, savedCount, pendingCount, onConnect, onLive, onNavigate }: Props) {
  const connected = Boolean(recorder);
  const canReconnect=!connected&&!busy&&!connecting&&!checking;
  const start=useRef<{x:number;y:number}|null>(null);
  const [pull,setPull]=useState(0);const pullRef=useRef(0);
  const [reconnecting,setReconnecting]=useState(false);
  const blockClickUntil=useRef(0);
  const resetPull=()=>{start.current=null;pullRef.current=0;setPull(0);};
  const finishPull=(cancelled=false)=>{
    const ready=pullRef.current>=56&&canReconnect&&!cancelled;
    if(pullRef.current>12)blockClickUntil.current=Date.now()+500;
    resetPull();
    if(ready){setReconnecting(true);void onReconnect().finally(()=>setReconnecting(false));}
  };
  return <div className="glacier-home" onPointerDown={()=>{blockClickUntil.current=0;}} onTouchStart={event=>{
    blockClickUntil.current=0;resetPull();
    if(!canReconnect || window.scrollY>1 || event.touches.length!==1 || (event.target as HTMLElement).closest('input,textarea,select'))return;
    start.current={x:event.touches[0].clientX,y:event.touches[0].clientY};
  }} onTouchMove={event=>{
    if(!start.current)return;
    if(!canReconnect||event.touches.length!==1){resetPull();return;}
    const dx=event.touches[0].clientX-start.current.x;const dy=event.touches[0].clientY-start.current.y;
    if(Math.abs(dx)>30 || dy<0){resetPull();return;}
    pullRef.current=Math.min(80,dy*.6);setPull(pullRef.current);
  }} onTouchEnd={event=>{if(pullRef.current>12&&event.cancelable)event.preventDefault();finishPull();}} onTouchCancel={()=>finishPull(true)} onClickCapture={event=>{
    if(Date.now()<blockClickUntil.current){event.preventDefault();event.stopPropagation();}
  }}>
    <div className="device-pull-connect" role="status" style={{height:reconnecting?36:pull}}>{(pull>12||reconnecting)&&<><RefreshCw aria-hidden="true" size={16} className={reconnecting?'spinning':''}/>{reconnecting?'正在连接…':pull>=56?'松开连接':''}</>}</div>
    <header className="glacier-home-heading">
      <button className="device-title-button" aria-label="修改记录仪标题" onClick={onTitle}><h1>{title}</h1></button>
      <button className="device-identity-details" aria-label="查看或修改车辆信息" onClick={onVehicle}><span>{vehicleInfo?.model||'添加车型'}</span>{vehicleInfo?.year&&<span>{vehicleInfo.year} 款</span>}{vehicleInfo?.configuration&&<strong>{vehicleInfo.configuration}</strong>}</button>
    </header>
    <section className="glacier-device" aria-label={vehicleName+'记录仪'}>
      <button className="tech-product-image glacier-product-stage" aria-label="更换记录仪图片" onClick={onImage}>
        <div className="glacier-orbit orbit-outer" aria-hidden="true" />
        <div className="glacier-orbit orbit-inner" aria-hidden="true" />
        <img src={image||'/images/glacier-device.webp'} alt="我的记录仪" />
      </button>
      <div className="vehicle-highlight-card">
        <div className="device-connection-summary"><span>原车记录仪</span><span className={'glacier-connection-state '+(connected?'online':'')}><i/>{connected?'已连接':checking?'连接中':'未连接'}</span></div>
        <button className="glacier-connect-button" disabled={busy || checking} onClick={connected ? () => onNavigate('videos') : onConnect}>
          {connected ? <Clapperboard size={20} /> : <Wifi size={21} />}<span>{connected ? '查看行车录像' : checking ? '正在连接…' : '连接记录仪'}</span><ChevronRight size={20} />
        </button>
      </div>
    </section>
    <section className="glacier-shortcuts" aria-label="快捷操作">
      <button disabled={!recorder || checking || recorder.profile==='L6'} onClick={onLive}><span className="glacier-shortcut-icon"><Camera size={22} /></span><span><strong>实时画面</strong><small>{recorder?.profile==='L6'?'暂不支持实时预览':'查看记录仪画面'}</small></span><ChevronRight size={17}/></button>
      <button onClick={() => onNavigate('videos')}><span className="glacier-shortcut-icon"><Video size={22} /></span><span><strong>行车录像</strong><small>{connected ? `共 ${videoCount} 段录像` : '查看与管理'}</small></span><ChevronRight size={17}/></button>
      <button onClick={() => onNavigate('transfers')}><span className="glacier-shortcut-icon"><ArrowDownUp size={23} /></span><span><strong>传输任务</strong><small>{pendingCount?`正在传输 ${pendingCount}`:'传输与保存'}</small></span><ChevronRight size={17}/></button>
      <button onClick={() => onNavigate('settings')}><span className="glacier-shortcut-icon"><Settings size={22} /></span><span><strong>设备设置</strong><small>{savedCount?`已保存 ${savedCount} 段`:'记录仪参数'}</small></span><ChevronRight size={17}/></button>
    </section>
    <button className="device-manage-connection" disabled={busy||connecting} onClick={onConnect}><span className="glacier-shortcut-icon"><Link2 size={20}/></span><span>管理连接</span><ChevronRight size={18}/></button>
  </div>;
}
