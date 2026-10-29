import { ArrowRight, Check, Download, Pause, Play, ShieldAlert, Trash2 } from 'lucide-react';
import type { Task } from './types';
import { formatSize } from './lib/recorder';

type Props = { task:Task; onPause:()=>void; onResume:()=>void; onDelete:()=>void; onOpenSaved:()=>void };

export default function TransferCard({task,onPause,onResume,onDelete,onOpenSaved}:Props) {
  const active=task.status==='waiting'||task.status==='downloading';
  const resumable=['failed','cancelled','paused'].includes(task.status);
  const state=task.status==='downloading'&&task.phase==='masking'?'正在打码':task.status==='downloading'&&task.phase==='saving'?'正在保存':{
    waiting:'等待下载',downloading:'正在下载',done:'已保存',cancelled:'已取消',failed:'下载中断',paused:'已暂停',
  }[task.status];
  const progress=task.status==='done'?100:task.total?Math.min(100,Math.round(task.received/task.total*100)):undefined;
  return <div className="transfer-card">
    <div className={'transfer-symbol '+task.status}>{task.status==='done'?<Check size={23}/>:task.status==='failed'?<ShieldAlert size={23}/>:<Download size={23}/>}</div>
    <div className="transfer-main">
      <div className="transfer-heading"><h2>{task.video.time||task.video.name}<span>{task.video.date}</span></h2><span className={'task-state '+task.status}>{state}{task.maskVin?' · 打码保存':''}</span></div>
      <div className="progress-track" role="progressbar" aria-label="下载进度" aria-valuemin={0} aria-valuemax={100} aria-valuenow={progress}><div style={{width:(progress??0)+'%'}}/></div>
      <div className="transfer-meta"><span>{task.error||(task.status==='done'?'已保存到系统相册':formatSize(task.received)+' / '+formatSize(task.total))}</span></div>
      <div className="transfer-task-actions">
        {active?<button className="transfer-task-action" onClick={onPause}><Pause size={15}/><span>暂停</span></button>
          :resumable?<button className="transfer-task-action" onClick={onResume}><Play size={15}/><span>继续下载</span></button>
          :<button className="transfer-task-action" onClick={onOpenSaved}><ArrowRight size={15}/><span>查看录像</span></button>}
        <button className="transfer-task-action transfer-task-delete" aria-label={'删除任务 '+task.video.name} onClick={onDelete}><Trash2 size={15}/><span>删除任务</span></button>
      </div>
    </div>
  </div>;
}
