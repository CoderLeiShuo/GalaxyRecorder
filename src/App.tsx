import { useEffect, useMemo, useRef, useState } from 'react';
import { ArrowDownToLine, ArrowDownUp, ArrowLeft, ArrowRight, Camera, CarFront, Check, CheckCheck, ChevronRight, CircleHelp, CirclePlay, Clock3, Download, Film, FolderOpen, Grid2X2, HardDrive, List, LoaderCircle, MoreVertical, Play, Plus, RefreshCw, Search, Settings, Settings2, ShieldCheck, ShieldAlert, Unplug, Wifi, X } from 'lucide-react';
import { listen } from '@tauri-apps/api/event';
import type { DownloadEvent, Filter, Page, Recorder, Task, Video } from './types';
import { cancelDownload, demoVideos, downloadVideo, formatDuration, formatSize, native, readSaved, readVideos, revealSaved, targetVehicle, videoSource, visibleVideos } from './lib/recorder';
import DeviceHome from './DeviceHome';
import ConnectDialog from './ConnectDialog';
import { invoke } from '@tauri-apps/api/core';

const nav = [
  { key: 'device' as const, label: '我的设备', short: '设备', icon: Camera },
  { key: 'videos' as const, label: '行车录像', short: '录像', icon: CirclePlay },
  { key: 'transfers' as const, label: '传输任务', short: '传输', icon: ArrowDownUp },
  { key: 'settings' as const, label: '设置与帮助', short: '设置', icon: Settings },
];
const sleep = (ms: number) => new Promise(resolve => setTimeout(resolve, ms));
const errorText = (error: unknown) => error instanceof Error ? error.message : String(error);

function App() {
  const [page, setPage] = useState<Page>('device');
  const [recorder, setRecorder] = useState<Recorder | null>(null);
  const [demo, setDemo] = useState(false);
  const [remote, setRemote] = useState<Video[]>([]);
  const [saved, setSaved] = useState<Video[]>([]);
  const [tasks, setTasks] = useState<Task[]>([]);
  const [filter, setFilter] = useState<Filter>('all');
  const [date, setDate] = useState('');
  const [query, setQuery] = useState('');
  const [view, setView] = useState<'grid' | 'list'>('grid');
  const [filterToolsOpen, setFilterToolsOpen] = useState(false);
  const [selecting, setSelecting] = useState(false);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [loading, setLoading] = useState(false);
  const [connectOpen, setConnectOpen] = useState(false);
  const [currentVideo, setCurrentVideo] = useState<Video | null>(null);
  const [message, setMessage] = useState<{text: string; error?: boolean} | null>(null);
  const working = useRef(false);
  const cancelled = useRef(new Set<string>());
  const connectionVersion = useRef(0);
  const busy = tasks.some(t => t.status === 'waiting' || t.status === 'downloading');

  useEffect(() => {
    if (!message || message.error) return;
    const timer = setTimeout(() => setMessage(null), 4500);
    return () => clearTimeout(timer);
  }, [message]);

  useEffect(() => {
    document.documentElement.dataset.theme = 'light';
    localStorage.setItem('galaxy-theme', 'light');
  }, []);
  useEffect(() => { readSaved().then(setSaved).catch(error => setMessage({text: errorText(error),error:true})); }, []);
  useEffect(() => {
    if (!native) return;
    let closed = false;
    let unsubscribe: (()=>void) | undefined;
    listen<DownloadEvent>('download-progress', ({payload}) => {
      setTasks(items => items.map(task => task.id === payload.taskId && task.status === 'downloading' ? {...task, received: payload.received,total: payload.total} : task));
    }).then(fn => { if(closed) fn(); else unsubscribe=fn; }).catch(error=>setMessage({text:errorText(error),error:true}));
    return () => { closed=true;unsubscribe?.(); };
  }, []);
  useEffect(() => {
    if (working.current) return;
    const task = tasks.find(item=>item.status==='waiting');
    if (!task) return;
    working.current=true;
    setTasks(items=>items.map(item=>item.id===task.id?{...item,status:'downloading'}:item));
    (async () => {
      try {
        let path: string;
        if(task.video.demo) {
          for(let progress=1;progress<=10;progress++) {
            await sleep(180);
            if(cancelled.current.has(task.id)) throw new Error('下载已取消');
            setTasks(items=>items.map(item=>item.id===task.id?{...item,received:task.video.size*progress/10,total:task.video.size}:item));
          }
          path='demo';
        } else {
          if(!recorder) throw new Error('请重新连接记录仪后重试');
          path=await downloadVideo(recorder.address,task.video,task.id);
        }
        setSaved(items=>[...items.filter(item=>item.id!==task.video.id),{...task.video,savedPath:path}]);
        setTasks(items=>items.map(item=>item.id===task.id?{...item,status:'done',savedPath:path,received:item.total||item.received}:item));
      } catch(error) {
        const isCancelled=cancelled.current.has(task.id);
        setTasks(items=>items.map(item=>item.id===task.id?{...item,status:isCancelled?'cancelled':'failed',error:errorText(error)}:item));
      } finally {
        cancelled.current.delete(task.id);
        working.current=false;
        // Trigger the next waiting job after releasing the serial queue lock.
        setTasks(items=>[...items]);
      }
    })();
  }, [tasks,recorder]);

  const videos = useMemo(() => {
    const map=new Map<string,Video>();
    (demo?demoVideos:remote).forEach(video=>map.set(video.id,video));
    saved.filter(video=>Boolean(video.demo)===demo).forEach(video=>map.set(video.id,video));
    return [...map.values()];
  },[demo,remote,saved]);
  const shown = useMemo(()=>visibleVideos(videos,filter,date,query),[videos,filter,date,query]);
  const notify = (text: string,error=false) => setMessage({text,error});

  async function load(rec: Recorder) {
    const version=++connectionVersion.current;
    setLoading(true);
    const result=await Promise.allSettled([readVideos(rec.address,'normal'),readVideos(rec.address,'event')]);
    if(version!==connectionVersion.current) return;
    const collected:Video[]=[];const warnings:string[]=[];
    result.forEach((value,index)=>{if(value.status==='fulfilled'){collected.push(...value.value.videos);if(value.value.warning)warnings.push(value.value.warning);}else warnings.push((index===0?'普通录像':'紧急录像')+'：'+errorText(value.reason));});
    setRemote(collected);setLoading(false);
    if(warnings.length)notify(warnings.join('；'),true);
    else notify(collected.length?'录像列表已更新':'设备未返回录像，接口仍需实车核对');
  }
  function startDemo() {
    if(busy) return;
    if(native) void invoke('release_recorder_network').catch(error=>notify(errorText(error),true));
    connectionVersion.current++;setLoading(false);setDemo(true);setRecorder(null);setRemote([]);setDate('');setFilter('all');setSelected(new Set());setConnectOpen(false);setPage('videos');
    notify('演示模式已开启：使用示例录像，不会连接车辆或保存真实文件');
  }
  function leaveDemo() {
    if(busy) return;
    setDemo(false);setFilter('all');setSelected(new Set());setMessage(null);setTasks(items=>items.filter(item=>!item.video.demo));
  }
  function enqueue(items:Video[]) {
    const eligible=items.filter(video=>!video.savedPath&&!tasks.some(task=>task.video.id===video.id&&['waiting','downloading','done'].includes(task.status)));
    if(!eligible.length){notify('所选录像已保存或已在传输队列中');return;}
    setTasks(current=>[...current,...eligible.map(video=>({id:crypto.randomUUID(),video,status:'waiting' as const,received:0,total:video.size||null}))]);
    setSelected(new Set());setSelecting(false);notify(demo?'已加入演示队列，不会保存真实文件':'已加入传输队列，完成后可在“已保存”中查看');
  }
  async function cancel(task:Task) {
    if(task.status==='waiting'){setTasks(items=>items.map(item=>item.id===task.id?{...item,status:'cancelled'}:item));return;}
    cancelled.current.add(task.id);
    if(!task.video.demo) await cancelDownload(task.id).catch(error=>notify(errorText(error),true));
  }
  function retry(task:Task) {
    if(!task.video.demo&&!recorder){notify('请先重新连接记录仪',true);setConnectOpen(true);return;}
    cancelled.current.delete(task.id);setTasks(items=>items.map(item=>item.id===task.id?{...item,status:'waiting',received:0,error:undefined}:item));
  }
  function toggleSelection(id:string) {setSelected(items=>{const next=new Set(items);if(next.has(id))next.delete(id);else next.add(id);return next;});}
  function switchPage(next:Page) {setPage(next);setSelected(new Set());setSelecting(false);}
  const connected = demo || Boolean(recorder);

  return <div className="app-shell"><div className="glacier-backdrop" aria-hidden="true"/>
    <aside className="sidebar">
      <button className="brand" onClick={()=>switchPage('videos')}><span className="brand-mark"><Camera size={22}/></span><span>银河记录仪<small>留住每一程</small></span></button>
      <div className="nav-label">工作空间</div>
      <nav aria-label="主导航">{nav.map(({key,label,short,icon:Icon})=><button key={key} className={'nav-item '+(page===key?'active':'')} aria-current={page===key?'page':undefined} onClick={()=>switchPage(key)}><Icon size={19}/><span className="nav-desktop-label">{label}</span><span className="nav-mobile-label">{short}</span>{key==='transfers'&&busy&&<span className="nav-count">{tasks.filter(t=>['waiting','downloading'].includes(t.status)).length}</span>}</button>)}</nav>
      <div className="sidebar-bottom"><div className="vehicle-mini"><span className="vehicle-icon"><CarFront size={20}/></span><div><strong>星越 L <small>2026</small></strong><span><i className={connected?'status-dot online':'status-dot'}/>{demo?'演示设备':recorder?'已连接记录仪':'等待连接'}</span></div></div><button className="quiet-button demo-launch" disabled={busy} onClick={demo?leaveDemo:startDemo}>{demo?<Unplug size={16}/>:<Play size={16}/>}<span>{demo?'退出演示模式':'体验界面演示'}</span></button><div className="sidebar-version">React + Tauri <span>v0.1.2</span></div></div>
    </aside>

    <div className="workspace" data-page={page}>
      <header className={'topbar topbar-'+page}><button className="mobile-brand" onClick={()=>switchPage('device')} aria-label="返回设备首页"><span className="mobile-brand-icon"><Camera size={20}/></span><span>银河记录仪<small>GALAXY RECORDER</small></span></button><div className="mobile-page-heading"><button className="icon-button" aria-label="返回设备首页" onClick={()=>switchPage('device')}><ArrowLeft size={22}/></button><strong>{nav.find(item=>item.key===page)?.label}</strong></div><div className="breadcrumb">我的记录仪 <ChevronRight size={13}/><span>{nav.find(item=>item.key===page)?.label}</span></div><div className="topbar-actions">{page==='videos'&&<div className="glacier-library-actions"><button className="icon-button" aria-label="搜索与筛选" aria-expanded={filterToolsOpen} onClick={()=>setFilterToolsOpen(!filterToolsOpen)}><Search size={20}/></button><button className="icon-button" aria-label={selecting?'完成选择':'批量选择'} aria-pressed={selecting} onClick={()=>{setSelecting(!selecting);setSelected(new Set());}}><CheckCheck size={20}/></button></div>}{demo&&<span className="demo-badge">演示数据</span>}<button className="icon-button help-button" aria-label="使用帮助" onClick={()=>switchPage('settings')}><CircleHelp size={20}/></button><button aria-label={demo?'演示连接':recorder?'设备已连接':'连接记录仪'} className={'connection-button '+(connected?'connected':'')} disabled={busy} onClick={()=>setConnectOpen(true)}>{page==='device'?<Plus size={20}/>:<Wifi size={17}/>}<span>{demo?'演示连接':recorder?'设备已连接':'连接记录仪'}</span><ChevronRight size={13}/></button></div></header>
      <main className="main-content">
        {message&&<div className={'notice '+(message.error?'notice-error':'')} role={message.error?'alert':'status'}><span>{message.text}</span><button className="icon-button" aria-label="关闭提示" onClick={()=>setMessage(null)}><X size={15}/></button></div>}

        {page==='videos'&&<>
          <div className={'page-heading '+(filterToolsOpen?'is-tools-open':'')}><div><div className="eyebrow">YOUR JOURNEY, PRESERVED</div><h1>行车录像<span className="heading-dot">.</span></h1><p>{connected?'那些路过的风景，和值得保留的瞬间。':'连接你的星越 L，把路上的每一刻留在身边。'}</p></div><div className="heading-actions"><button className="secondary-button" disabled={!connected||loading} onClick={()=>demo?notify('示例录像已是最新'):recorder&&load(recorder)}><RefreshCw size={16} className={loading?'spinning':''}/><span>{loading?'读取中':'刷新录像'}</span></button>{videos.length>0&&<button className={'secondary-button '+(selecting?'selected-button':'')} onClick={()=>{setSelecting(!selecting);setSelected(new Set());}}><CheckCheck size={16}/><span>{selecting?'完成选择':'批量选择'}</span></button>}</div></div>
          <button type="button" className="glacier-recorder-strip" disabled={busy} onClick={()=>setConnectOpen(true)} aria-label="管理记录仪连接"><img src="/images/glacier-device.png" alt=""/><div><strong>星越 L<small>2026 款</small></strong><span className="subtle-text"><i className={connected?'status-dot online':'status-dot'}/>{demo?'演示设备':connected?'已连接':'未连接'}</span></div><ChevronRight size={19}/></button>
          <div className="library-toolbar"><div className="filter-tabs" aria-label="录像分类">{([['all','全部录像'],['normal','普通录像'],['event','紧急录像'],['saved','已保存']] as const).map(([key,label])=><button key={key} aria-pressed={filter===key} className={filter===key?'active':''} onClick={()=>{setFilter(key);setSelected(new Set());}}>{label}{key==='saved'&&saved.filter(v=>Boolean(v.demo)===demo).length>0&&<span>{saved.filter(v=>Boolean(v.demo)===demo).length}</span>}</button>)}</div><div className={'toolbar-controls '+(filterToolsOpen?'is-open':'')}><label className="search-field"><Search size={15}/><input aria-label="搜索录像" placeholder="搜索录像" value={query} onChange={event=>setQuery(event.target.value)}/></label><input className="date-input" type="date" aria-label="按日期筛选" value={date} onChange={event=>setDate(event.target.value)}/>{date&&<button className="icon-button" aria-label="清除日期" onClick={()=>setDate('')}><X size={14}/></button>}<div className="view-toggle"><button className={view==='grid'?'active':''} aria-label="网格视图" aria-pressed={view==='grid'} onClick={()=>setView('grid')}><Grid2X2 size={16}/></button><button className={view==='list'?'active':''} aria-label="列表视图" aria-pressed={view==='list'} onClick={()=>setView('list')}><List size={17}/></button></div></div></div>
          {loading&&<div className="loading-state" role="status"><LoaderCircle className="spinning" size={20}/>正在读取记录仪录像，请稍候…</div>}
          {!loading&&shown.length>0&&<><div className="date-heading"><h2>{date||shown[0].date||'记录仪录像'}<span>{shown.length} 段录像</span></h2>{selecting&&<button className="text-button" onClick={()=>setSelected(selected.size===shown.length?new Set():new Set(shown.map(video=>video.id)))}>{selected.size===shown.length?'清空选择':'选择全部'}</button>}</div><div className={'video-collection '+view}>{shown.map((video,index)=><VideoCard key={video.id} video={video} index={index} selecting={selecting} selected={selected.has(video.id)} onSelect={()=>toggleSelection(video.id)} onOpen={()=>setCurrentVideo(video)} onSave={()=>enqueue([video])}/>)}</div></>}
          {!loading&&shown.length===0&&<div className="empty-state"><span className="empty-icon"><Film size={27}/></span><h2>{filter==='saved'?'还没有保存的录像':date||query?'没有符合条件的录像':connected?'还没有读取到录像':'录像将在这里等你'}</h2><p>{filter==='saved'?'把想要留住的片段，保存到应用媒体库。':date||query?'试试清除筛选条件，查看其他录像。':connected?'刷新列表，或检查车机记录仪是否正常工作。':'普通录像、紧急录像和已保存片段，都能轻松找到。'}</p>{(date||query)&&<button className="text-button" onClick={()=>{setDate('');setQuery('');}}>清除筛选条件</button>}</div>}
          {selected.size>0&&<div className="selection-bar"><span><Check size={16}/>已选 {selected.size} 段录像</span><div><button className="quiet-button" onClick={()=>setSelected(new Set())}>取消</button><button className="primary-button" onClick={()=>enqueue(videos.filter(video=>selected.has(video.id)))}><Download size={16}/>保存到媒体库</button></div></div>}
        </>}

        {page==='device'&&<DeviceHome recorder={recorder} demo={demo} busy={busy} videoCount={videos.length} savedCount={saved.filter(video=>Boolean(video.demo)===demo).length} pendingCount={tasks.filter(task=>['waiting','downloading'].includes(task.status)).length} onConnect={()=>setConnectOpen(true)} onDemo={startDemo} onNavigate={switchPage}/>}

        {page==='transfers'&&<><div className="page-heading"><div><div className="eyebrow">KEEP WHAT MATTERS</div><h1>传输任务<span className="heading-dot">.</span></h1><p>为值得留住的瞬间，留一份原画质。</p></div><span className="status-pill">{tasks.filter(task=>task.status==='done').length} 项已完成</span></div>{tasks.length===0?<div className="empty-state large"><span className="empty-icon"><ArrowDownToLine size={28}/></span><h2>暂时没有传输任务</h2><p>从行车录像中选择片段，保存到你的媒体库。</p><button className="primary-button" onClick={()=>switchPage('videos')}>去选择录像<ArrowRight size={16}/></button></div>:<div className="transfer-list">{tasks.map(task=><div className="transfer-card" key={task.id}><div className={'transfer-symbol '+task.status}>{task.status==='done'?<Check size={23}/>:task.status==='failed'?<ShieldAlert size={23}/>:<Download size={23}/>}</div><div className="transfer-main"><div className="transfer-heading"><h2>{task.video.time||task.video.name}<span>{task.video.date}</span></h2><span className={'task-state '+task.status}>{{waiting:'等待下载',downloading:'正在下载',done:'已保存',cancelled:'已取消',failed:'下载失败'}[task.status]}{task.video.demo?' · 演示':''}</span></div><div className="progress-track" role="progressbar" aria-label="下载进度" aria-valuemin={0} aria-valuemax={100} aria-valuenow={task.status==='done'?100:task.total?Math.round(task.received/task.total*100):undefined}><div style={{width:(task.status==='done'?100:task.total?Math.min(100,task.received/task.total*100):0)+'%'}}/></div><div className="transfer-meta"><span>{task.error|| (task.status==='done'?(task.video.demo?'示例任务完成，未保存真实文件':'已保存到应用媒体库'):formatSize(task.received)+' / '+formatSize(task.total))}</span>{['waiting','downloading'].includes(task.status)?<button className="text-button" onClick={()=>cancel(task)}>取消</button>:['failed','cancelled'].includes(task.status)?<button className="text-button" onClick={()=>retry(task)}><RefreshCw size={13}/>重试</button>:<button className="text-button" onClick={()=>{setFilter('saved');switchPage('videos');}}>查看录像<ArrowRight size={13}/></button>}</div></div></div>)}</div>}</>}

        {page==='settings'&&<><div className="page-heading"><div><div className="eyebrow">MAKE IT YOURS</div><h1>设置与帮助<span className="heading-dot">.</span></h1><p>简单一点，顺手一点。</p></div></div><div className="settings-layout"><section className="surface-panel"><div className="section-header"><h2>应用偏好</h2><Settings2 size={17}/></div><div className="setting-row"><div><strong>界面外观</strong><span>瓷白、银灰与冰蓝，清晰舒适。</span></div><span className="setting-value">浅色科技风</span></div><div className="setting-row"><div><strong>车架号打码</strong><span>正在适配中，敬请期待</span></div><span className="feature-status">尚未接入</span></div><div className="setting-row"><div><strong>片段截取</strong><span>保留需要的几秒钟，减少整段下载</span></div><span className="feature-status">尚未接入</span></div></section><section className="surface-panel"><div className="section-header"><h2>媒体与设备</h2><HardDrive size={17}/></div><div className="setting-row"><div><strong>本地媒体库</strong><span>{saved.filter(video=>!video.demo).length} 段录像已保存</span></div><button className="text-button" onClick={()=>{setFilter('saved');switchPage('videos');}}>查看<ChevronRight size={15}/></button></div>{native&&!/Android|iPhone|iPad/i.test(navigator.userAgent)&&<div className="setting-row"><div><strong>保存位置</strong><span>打开本机媒体库文件夹</span></div><button className="icon-button" aria-label="打开保存位置" onClick={()=>revealSaved().catch(error=>notify(errorText(error),true))}><FolderOpen size={18}/></button></div>}<div className="setting-row"><div><strong>车型适配</strong><span>先完成星越 L，再逐步扩展其他车型</span></div><span className="setting-value">2026 星越 L</span></div><div className="setting-row"><div><strong>录音与存储卡管理</strong><span>控制指令将在实车验证后开放</span></div><span className="feature-status">尚未接入</span></div></section><section className="surface-panel help-panel"><div className="section-header"><h2><CircleHelp size={17}/>连接小贴士</h2></div><ol><li>电脑或手机连接到车机无线热点。</li><li>必要时，在车机打开一次原厂记录仪应用。</li><li>输入记录仪地址，或搜索当前热点网段。</li></ol><p>手机连接热点时，请保持车机记录仪开启。当前录像保存在应用媒体库，相册导出正在适配中。</p></section><section className="surface-panel about-panel"><span className="brand-mark"><Camera size={22}/></span><div><h2>银河记录仪</h2><p>React + Tauri 2 · v0.1.2</p><span>首个适配目标：{targetVehicle}</span></div><button className="quiet-button" disabled={busy} onClick={demo?leaveDemo:startDemo}><Play size={15}/>{demo?'退出演示':'体验演示'}</button></section></div></>}
      </main>
      <footer className="workspace-footer"><span><ShieldCheck size={13}/>{demo?'演示模式 · 非实车数据':'你的录像，留在你的设备里'}</span><span>{native?'Tauri 原生应用':'界面预览'} · {targetVehicle}</span></footer>
    </div>
    {connectOpen&&<ConnectDialog busy={busy} recorder={recorder} demo={demo} onClose={()=>setConnectOpen(false)} onDemo={startDemo} onDisconnect={()=>{if(busy)return;if(native)void invoke('release_recorder_network').catch(error=>notify(errorText(error),true));connectionVersion.current++;setRecorder(null);setRemote([]);setLoading(false);setDemo(false);setConnectOpen(false);notify('已断开记录仪连接');}} onConnect={rec=>{if(busy)return;setDemo(false);setRecorder(rec);setFilter('all');setDate('');setConnectOpen(false);setPage('videos');void load(rec);}}/>}
    {currentVideo&&<VideoDialog video={currentVideo} onClose={()=>setCurrentVideo(null)} onSave={()=>{enqueue([currentVideo]);setCurrentVideo(null);}} onMessage={notify}/>}
  </div>;
}

function VideoCard({video,index,selecting,selected,onSelect,onOpen,onSave}:{video:Video;index:number;selecting:boolean;selected:boolean;onSelect:()=>void;onOpen:()=>void;onSave:()=>void}) {
  return <article className={'video-card '+(selected?'is-selected':'')}><button className={'video-image '+(video.demo?'scene scene-'+index%4:'video-placeholder')} onClick={selecting?onSelect:onOpen} aria-label={(selecting?'选择':'查看')+' '+(video.time||video.name)+' 的录像'}>{video.demo?<><img className="demo-road-image" src="/images/demo-highway.png" alt="示例旅程画面"/><span className="scene-watermark">演示画面</span></>:<Film size={32} strokeWidth={1.2}/>}<span className="video-kind">{video.category==='event'?<ShieldAlert size={11}/>:<Camera size={11}/>} {video.category==='event'?'紧急录像':'普通录像'}</span>{selecting?<span className={'video-checkbox '+(selected?'checked':'')} role="checkbox" aria-checked={selected}>{selected&&<Check size={13}/>}</span>:<span className="play-overlay"><Play size={20} fill="currentColor"/></span>}<span className="video-duration">{formatDuration(video.duration)}</span></button><div className="video-info"><div><button className="video-title" onClick={onOpen}>{video.time||video.name}</button><span className={'video-category-inline '+(video.category==='event'?'event':'')}>{video.category==='event'?'紧急':'普通'}</span><p>{video.date||'日期待读取'}<span>·</span>{formatSize(video.size)}</p></div><button className={'save-button '+(video.savedPath?'saved':'')} disabled={Boolean(video.savedPath)} onClick={onSave} aria-label={video.savedPath?'已保存':'保存 '+video.time+' 的录像'}>{video.savedPath?<Check size={17}/>:<Download size={17}/>}<span>{video.savedPath?'已存':'保存'}</span></button><button className="video-more-button" aria-label="查看录像详情" onClick={onOpen}><MoreVertical size={15}/></button></div></article>;
}

function Modal({children,onClose,label}:{children:React.ReactNode;onClose:()=>void;label:string}) {
  const dialog=useRef<HTMLDialogElement>(null);
  useEffect(()=>{const element=dialog.current;element?.showModal();return()=>element?.close();},[]);
  return <dialog ref={dialog} className="modal" aria-label={label} onCancel={onClose} onClick={event=>{if(event.target===event.currentTarget)onClose();}}><div className="modal-content">{children}</div></dialog>;
}

function VideoDialog({video,onClose,onSave,onMessage}:{video:Video;onClose:()=>void;onSave:()=>void;onMessage:(message:string)=>void}) {
  const source=videoSource(video);
  return <Modal label="录像详情" onClose={onClose}><div className="player-heading"><button className="icon-button" aria-label="返回录像" onClick={onClose}><ArrowLeft size={20}/></button><div><strong>{video.time||video.name}</strong><span>{video.date} · {video.category==='event'?'紧急录像':'普通录像'}</span></div><span className="feature-status">{video.demo?'画面示意':'录像详情'}</span></div><div className="player-area">{source?<video src={source} controls autoPlay playsInline onError={()=>onMessage('系统无法播放此录像，可在保存目录中使用其他播放器打开')}/>:<><Film size={44} strokeWidth={1}/><strong>{video.demo?'录像画面示意':'保存后即可播放'}</strong><span>{video.demo?'这里使用示例数据，未加载真实视频':'保存到媒体库后，即可打开录像。在线回看正在适配中。'}</span></>}</div><div className="player-details"><span><Clock3 size={14}/>{formatDuration(video.duration)}</span><span><HardDrive size={14}/>{formatSize(video.size)}</span><span><ShieldCheck size={14}/>原画质</span></div><div className="player-note"><ShieldCheck size={18}/><div><strong>隐私导出</strong><p>车架号打码与片段截取正在适配中。当前将保存完整的原始录像。</p></div></div><button className="primary-button full-width" disabled={Boolean(video.savedPath)} onClick={onSave}>{video.savedPath?<Check size={17}/>:<Download size={17}/>} {video.savedPath?'已保存到媒体库':'保存完整录像'}</button></Modal>;
}

export default App;


