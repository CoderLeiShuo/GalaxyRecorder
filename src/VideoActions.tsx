import { useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Download, FolderOpen, LoaderCircle, Pencil, Scissors, Share2, Trash2, X } from 'lucide-react';
import type { AppSettings, Video } from './types';
import Dialog from './Dialog';
import { mobile, getSettings } from './lib/media';
import { formatDuration, formatSize } from './lib/recorder';

export default function VideoActions({ video, onClose, onEdit, onSave, onChanged, onDeleted }: { video: Video; onClose: () => void; onEdit: () => void; onSave: () => Promise<string>; onChanged: (video: Video) => void; onDeleted: () => void }) {
 const [working,setWorking]=useState('');const [message,setMessage]=useState('');const [error,setError]=useState('');
 const [confirm,setConfirm]=useState(false);const [renaming,setRenaming]=useState(false);const [name,setName]=useState(video.name);
 const saved=Boolean(video.savedPath);const gallery=video.storage==='gallery';
 async function run(action:string,task:()=>Promise<void>){setWorking(action);setError('');setMessage('');try{await task();}catch(error){setError(String(error));}finally{setWorking('');}}
 async function saveTo(mode:'gallery'|'folder') {
  if(mode==='folder') {const before=await getSettings();if(!before.directory){const selected=await invoke<AppSettings|null>('choose_download_directory');if(!selected)return;if(before.saveMode!=='folder')await invoke('save_settings',{settings:{...selected,saveMode:before.saveMode}});}}
  const updated=await invoke<Video>('export_saved',{id:video.id,mode});onChanged(updated);setMessage(mode==='gallery'?'已保存到相册':'已保存到指定文件夹');
 }
 return <Dialog label="录像文件操作" className="video-actions-drawer" onClose={()=>{if(!working)onClose();}}><div className="modal-content">
  <div className="modal-heading"><div><span className="drawer-eyebrow">录像文件</span><h2>{video.name}</h2></div><button className="icon-button" aria-label="关闭录像菜单" disabled={Boolean(working)} onClick={onClose}><X size={21}/></button></div>
  <div className="file-details"><span>{video.date} {video.time}</span><span>{formatSize(video.size)} · {formatDuration(video.duration)}</span><span>{saved?gallery?'保存在系统相册':video.storage==='folder'?'保存在指定文件夹':'以前保存的录像':'记录仪原片'}</span></div>
  {error&&<p className="form-error" role="alert">{error}</p>}{message&&<p className="drawer-message" role="status">{message}</p>}
  {working&&<p className="connection-progress" role="status"><LoaderCircle className="spinning" size={15}/>正在处理…</p>}
  {!confirm&&!renaming&&<div className="file-action-list">
   {saved?<><button disabled={Boolean(working)} onClick={()=>void run('share',async()=>{await invoke('share_saved_many',{ids:[video.id],original:false});setMessage(mobile?'已打开系统分享':'已打开文件位置');})}><Share2 size={19}/><span>{mobile?'分享视频':'打开文件位置'}</span></button>
    {mobile&&<button disabled={Boolean(working)} onClick={()=>void run('original',async()=>{await invoke('share_saved_many',{ids:[video.id],original:true});setMessage('已打开原文件分享');})}><Share2 size={19}/><span>分享原文件</span><small>保留原画质</small></button>}
    <button disabled={Boolean(working)} onClick={()=>{setName(video.name);setRenaming(true);}}><Pencil size={19}/><span>重命名</span></button>
    {!gallery&&<button disabled={Boolean(working)} onClick={()=>void run('gallery',()=>saveTo('gallery'))}><Download size={19}/><span>保存到相册</span></button>}
    <button disabled={Boolean(working)} onClick={()=>void run('folder',()=>saveTo('folder'))}><FolderOpen size={19}/><span>另存到文件夹</span></button>
   </>:<button disabled={Boolean(working)} onClick={()=>void run('download',async()=>{setMessage(await onSave());})}><Download size={19}/><span>保存到相册</span></button>}
   <button disabled={Boolean(working)} onClick={onEdit}><Scissors size={19}/><span>截取与打码</span></button>
   {saved&&<button className="danger-text" disabled={Boolean(working)} onClick={()=>setConfirm(true)}><Trash2 size={19}/><span>删除录像文件</span></button>}
  </div>}
  {renaming&&<form className="file-action-form" onSubmit={event=>{event.preventDefault();void run('rename',async()=>{const updated=await invoke<Video>('rename_saved',{id:video.id,name});onChanged(updated);setRenaming(false);setMessage('文件已重命名');});}}><label className="form-field">文件名<input aria-label="录像文件名" autoFocus maxLength={128} value={name} disabled={Boolean(working)} onChange={event=>setName(event.target.value)}/></label><div><button type="button" className="secondary-button" disabled={Boolean(working)} onClick={()=>setRenaming(false)}>取消</button><button className="primary-button" disabled={Boolean(working)||!name.trim()}>保存名称</button></div></form>}
  {confirm&&<div className="file-delete-confirm"><h3>删除这段录像？</h3><p>{gallery?'会从系统相册中删除该文件。':video.storage==='folder'?'会从保存文件夹中删除该文件。':'会删除这段以前保存的本地文件。'}记录仪原片保留。</p><div><button className="secondary-button" disabled={Boolean(working)} onClick={()=>setConfirm(false)}>取消删除</button><button className="primary-button danger-button" disabled={Boolean(working)} onClick={()=>void run('delete',async()=>{await invoke('delete_saved',{ids:[video.id]});onDeleted();onClose();})}>确认删除文件</button></div></div>}
 </div></Dialog>;
}
