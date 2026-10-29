import { useEffect, useMemo, useRef, useState } from 'react';
import { ArrowDownToLine, ArrowDownUp, ArrowLeft, ArrowRight, Camera, CarFront, Check, CheckCheck, ChevronRight, CircleHelp, CirclePlay, Download, Film, Grid2X2, List, LoaderCircle, Trash2, RefreshCw, Search, Settings, ShieldCheck, Wifi, X } from 'lucide-react';
import { listen } from '@tauri-apps/api/event';
import type { Filter, Page, Recorder, Task, Video } from './types';
import { native, readSaved, readVideoLibrary, visibleVideos, groupVideosByDate } from './lib/recorder';
import VehicleDialog from './VehicleDialog';
import {readDeviceMemory,vehicleName,validProfile} from './lib/vehicle';
import type {DeviceMemory,ConnectionProfile,ConnectOptions} from './types';
import DeviceHome from './DeviceHome';
import {appName,displayAppName} from './lib/branding';
import AppearanceEditor from './AppearanceEditor';
import {readAppearance,appearanceFromSettings} from './lib/appearance';
import ConnectDialog from './ConnectDialog';
import { invoke } from '@tauri-apps/api/core';
import Player from './Player';
import VideoCard from './VideoCard';
import TransferCard from './TransferCard';
import SettingsPage from './Settings';
import Dialog from './Dialog';
import { clearMetadataCache } from './lib/media';
import { monitorConnection, type ConnectionHealth } from './lib/connectionMonitor';
import BatchActions from './BatchActions';
import VideoActions from './VideoActions';
import { getSettings } from './lib/media';
import type { AppSettings } from './types';

const nav = [
  { key: 'device' as const, label: '我的设备', short: '设备', icon: Camera },
  { key: 'videos' as const, label: '行车录像', short: '录像', icon: CirclePlay },
  { key: 'transfers' as const, label: '传输任务', short: '传输', icon: ArrowDownUp },
  { key: 'settings' as const, label: '设置与帮助', short: '设置', icon: Settings },
];
const errorText = (error: unknown) => error instanceof Error ? error.message : String(error);

function App() {
  const [lastDevice,setLastDevice]=useState<DeviceMemory|null>(()=>readDeviceMemory(localStorage.getItem('galaxy-device')));
  const rememberedDevice=useRef(lastDevice);rememberedDevice.current=lastDevice;
  const [connectionProfile,setConnectionProfile]=useState<ConnectionProfile>(()=>validProfile(localStorage.getItem('galaxy-connection-profile')));
  const selectedProfile=useRef(connectionProfile);selectedProfile.current=connectionProfile;
  const preferencesReady=useRef<Promise<unknown>>(Promise.resolve());
  const [vehicleOpen,setVehicleOpen]=useState(false);
  const [displayName,setDisplayName]=useState(()=>displayAppName(localStorage.getItem('galaxy-display-name')));
  const [nameOpen,setNameOpen]=useState(false);const [nameDraft,setNameDraft]=useState('');const [nameError,setNameError]=useState('');const [nameWorking,setNameWorking]=useState(false);
  const [actionVideo,setActionVideo]=useState<Video|null>(null);
  const [playerEdit,setPlayerEdit]=useState(false);const [playerSequence,setPlayerSequence]=useState(0);
  const [page, setPage] = useState<Page>('device');
  const [recorder, setRecorder] = useState<Recorder | null>(null);
  const [connectionChecking, setConnectionChecking] = useState(false);
  const [connecting, setConnecting] = useState(false);
  const connectRequest = useRef<Promise<Recorder> | null>(null);
  const connectGeneration = useRef(0);
  const releasing=useRef<Promise<unknown>>(Promise.resolve());
  const releaseConnection=()=>{const request=invoke('release_recorder_network').catch(()=>{});releasing.current=request;return request;};
  const [appearance,setAppearance]=useState(readAppearance);
  const [appearanceEditor,setAppearanceEditor]=useState<'title'|'vehicle'|'image'|null>(null);
  const homeVehicle=lastDevice&&appearance.vehicleAliases[lastDevice.key]||vehicleName(lastDevice);
  const vehicleAlias=lastDevice&&appearance.vehicleAliases[lastDevice.key];
  const homeTitle=appearance.deviceTitle!=='我的记录仪'?appearance.deviceTitle:`${vehicleAlias||lastDevice?.vehicle?.model||'我的记录仪'} · ${displayName}`;
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
  const [liveOpen, setLiveOpen] = useState(false);
  const [visibleLimit, setVisibleLimit] = useState(60);
  const [metadataVersion, setMetadataVersion] = useState(0);
  const [deleteConfirm, setDeleteConfirm] = useState(false);
  const [batchVideos, setBatchVideos] = useState<Video[] | null>(null);
  const [queueWorking, setQueueWorking] = useState(false);
  const savedRefresh=useRef(0);
  const completed = useRef(new Set<string>());
  const [message, setMessage] = useState<{text: string; error?: boolean} | null>(null);
  const connectionVersion = useRef(0);
  const busy = tasks.some(t => t.status === 'waiting' || t.status === 'downloading');
  const context = useRef({ recorder, busy }); context.current = { recorder, busy };

  useEffect(()=>{document.title=displayName;localStorage.setItem('galaxy-display-name',displayName);},[displayName]);
  useEffect(()=>{preferencesReady.current=getSettings().then(settings=>{if(settings.displayName)setDisplayName(displayAppName(settings.displayName));if(native)setAppearance(appearanceFromSettings(settings));if(settings.connectionProfile){selectedProfile.current=validProfile(settings.connectionProfile);setConnectionProfile(selectedProfile.current);}if(settings.lastDevice)setLastDevice(settings.lastDevice);}).catch(()=>{});},[]);
  useEffect(()=>{if(lastDevice)localStorage.setItem('galaxy-device',JSON.stringify(lastDevice));},[lastDevice]);
  useEffect(()=>{localStorage.setItem('galaxy-connection-profile',connectionProfile);},[connectionProfile]);
  async function chooseProfile(profile:ConnectionProfile){if(native)await invoke('set_connection_profile',{profile});selectedProfile.current=profile;setConnectionProfile(profile);}
  function vehicleSaved(device:DeviceMemory){rememberedDevice.current=device;setLastDevice(device);setRecorder(previous=>{if(previous?.deviceKey!==device.key)return previous;const next={...previous,vehicleInfo:device.vehicle,vehicle:device.vehicle?.name||'我的车辆'};localStorage.setItem('galaxy-session',JSON.stringify(next));return next;});notify('车型已保存');}
  function closeVehicle(){if(lastDevice)localStorage.setItem('galaxy-vehicle-dismissed:'+lastDevice.key,'1');setVehicleOpen(false);}

  function editAppName(){setNameDraft(displayName);setNameError('');setNameOpen(true);}
  async function saveAppName(){
    const name=nameDraft.trim();if(!name||[...name].length>32||/[\x00-\x1f\x7f]/.test(name)){setNameError('请输入 1–32 个可见字符');return;}
    setNameWorking(true);try{if(native){const result=await invoke<AppSettings>('rename_app',{name});setDisplayName(result.displayName||name);}else setDisplayName(name);setNameOpen(false);notify('名称已更新');}catch(error){setNameError(errorText(error));}finally{setNameWorking(false);}
  }
  function openVideo(video:Video,edit=false){setPlayerEdit(edit);setPlayerSequence(n=>n+1);setCurrentVideo(video);}
  function changedVideo(video:Video){savedRefresh.current++;setSaved(items=>[...items.filter(item=>item.id!==video.id),video]);setActionVideo(previous=>previous?.id===video.id?video:previous);setCurrentVideo(previous=>previous?.id===video.id?video:previous);}
  useEffect(() => {
    if (!message || message.error) return;
    const timer = setTimeout(() => setMessage(null), 4500);
    return () => clearTimeout(timer);
  }, [message]);

  useEffect(() => {
    document.documentElement.dataset.theme = 'light';
    localStorage.setItem('galaxy-theme', 'light');
  }, []);
  useEffect(()=>{
    let closed=false;
    const refresh=()=>{if(document.visibilityState==='hidden')return;const request=++savedRefresh.current;void readSaved().then(items=>{if(!closed&&request===savedRefresh.current)setSaved(items);}).catch(error=>{if(!closed)setMessage({text:errorText(error),error:true});});};
    refresh();window.addEventListener('focus',refresh);document.addEventListener('visibilitychange',refresh);
    return()=>{closed=true;savedRefresh.current++;window.removeEventListener('focus',refresh);document.removeEventListener('visibilitychange',refresh);};
  },[]);
  useEffect(() => {
    if (!native) return;
    const timer = setTimeout(() => { if(connectGeneration.current===0)void connectDevice().catch(() => {}); }, 250);
    return () => clearTimeout(timer);
  }, []);

  useEffect(() => {
    if(!native)return;
    let closed=false;let off:(()=>void)|undefined;
    listen<{kind:string}>('recorder-network',({payload})=>{
      if(closed || payload.kind!=='lost' || context.current.recorder || !connectRequest.current)return;
      stopConnecting();void releaseConnection();
    }).then(fn=>{if(closed)fn();else off=fn;}).catch(()=>{});
    return()=>{closed=true;off?.();};
  }, []);

  function stopConnecting() {
    connectGeneration.current++; setConnecting(false); setConnectionChecking(false); connectRequest.current=null;
    if (native) void invoke('cancel_recorder_discovery').catch(() => {});
  }
  function connectDevice(options:ConnectOptions={}): Promise<Recorder> {
    if (connectRequest.current) return connectRequest.current;
    if (!native) return Promise.reject(Error('真实连接请使用手机或电脑应用'));
    if (context.current.busy) return Promise.reject(Error('请先暂停下载'));
    const version=++connectGeneration.current;
    const request=(async () => {
      setConnecting(true); if (!context.current.recorder) setConnectionChecking(true);
      const ensureCurrent=() => { if(version!==connectGeneration.current) throw Error('连接已取消'); };
      try {
        await releasing.current; await preferencesReady.current; ensureCurrent();
        const profile=options.profile||selectedProfile.current;
        let rec:Recorder | null=context.current.recorder;
        if (!rec) { try { rec=JSON.parse(localStorage.getItem('galaxy-session') || 'null'); } catch { /* Ignore outdated session data. */ } }
        if(options.force||options.address)rec=null;
        if(rec&&profile!=='AUTO'&&rec.profile&&rec.profile!==({GENERIC1:'E5',GENERIC2:'A7',GENERIC3:'L6'} as Record<string,string>)[profile]&&rec.profile!==profile)rec=null;
        if (rec) {
          const health=await invoke<ConnectionHealth>('check_recorder_connection',{address:rec.address,networkId:rec.networkId??null,profile:rec.profile||profile}).catch(()=>({connected:false}));
          ensureCurrent(); if (!health.connected) rec=null;else if((health as ConnectionHealth&{recorder?:Recorder}).recorder)rec=(health as ConnectionHealth&{recorder:Recorder}).recorder;
        }
        if (!rec) {
          // Binding is refreshed before probing the remembered device; discovery is a fallback.
          const preferred=options.address||localStorage.getItem('galaxy-address');
          if (preferred) rec=await invoke<Recorder>('probe_recorder',{address:preferred,profile}).catch(()=>null);
          ensureCurrent();
          if(!rec&&options.address)throw Error('此地址未连接成功，请检查地址或切换连接方案');
          if (!rec) rec=await invoke<Recorder>('discover_recorder',{preferred,profile});
        }
        ensureCurrent();
        localStorage.setItem('galaxy-address',rec.address); localStorage.setItem('galaxy-session',JSON.stringify(rec));
        if(rec.deviceKey){const memory:DeviceMemory={key:rec.deviceKey,address:rec.address,profile:rec.profile||profile,vehicle:rememberedDevice.current?.key===rec.deviceKey&&rememberedDevice.current.vehicle?.source==='confirmed'&&(!rec.vehicleInfo||rec.vehicleInfo.model===rememberedDevice.current.vehicle.model)?rememberedDevice.current.vehicle:rec.vehicleInfo||(rememberedDevice.current?.key===rec.deviceKey?rememberedDevice.current.vehicle:null)};setLastDevice(memory);if(!memory.vehicle?.year&&!localStorage.getItem('galaxy-vehicle-dismissed:'+memory.key))setVehicleOpen(true);}
        setConnectionChecking(false);
        if(context.current.recorder?.address!==rec.address||context.current.recorder?.networkId!==rec.networkId)setRemote([]);setRecorder(rec);
        void load(rec); return rec;
      } catch (error) {
        if (version===connectGeneration.current) {
          localStorage.removeItem('galaxy-session'); connectionVersion.current++; setRecorder(null);setRemote([]);setLoading(false);
        }
        throw error;
      } finally { if(version===connectGeneration.current) {setConnecting(false);setConnectionChecking(false);connectRequest.current=null;} }
    })();
    connectRequest.current=request; return request;
  }
  async function reconnectFromPull() {
    if(context.current.recorder||context.current.busy||connectRequest.current)return;
    try {await connectDevice({force:true});notify('已连接记录仪');}
    catch {notify('连接未成功，请连接车机热点后再下拉重试',true);}
  }
  useEffect(() => {
    if (!native || !recorder) return;
    const monitor = monitorConnection({
      check: () => invoke<ConnectionHealth>('check_recorder_connection', { address: recorder.address, networkId: recorder.networkId ?? null }),
      checking: () => {},
      ready: () => setConnectionChecking(false),
      lost: reason => {
        connectGeneration.current++; connectRequest.current=null;setConnecting(false);localStorage.removeItem('galaxy-session'); connectionVersion.current++; setRecorder(null); setConnectionChecking(false); setRemote([]); setLoading(false);
        setSelected(new Set()); setSelecting(false); setLiveOpen(false); clearMetadataCache();
        setCurrentVideo(video => video?.savedPath ? video : null);
        setMessage({ text: reason, error: true });
        void releaseConnection();
      },
    });
    let closed = false; let unsubscribe: (() => void) | undefined;
    listen<{ kind: string; networkId?: string; reason?: string }>('recorder-network', ({ payload }) => {
      if (closed) return;
      if (payload.kind === 'lost') {
        if (payload.networkId && recorder.networkId && payload.networkId !== recorder.networkId) return;
        monitor.lost(payload.reason || '车机 Wi-Fi 已断开，请重新连接记录仪');
      } else if (payload.kind === 'pause') monitor.suspend();
      else if (payload.kind === 'resume') monitor.resume();
    }).then(fn => { if (closed) fn(); else unsubscribe = fn; }).catch(() => {});
    const visibility = () => document.visibilityState === 'hidden' ? monitor.suspend() : monitor.resume();
    const focus = () => { if (document.visibilityState !== 'hidden') monitor.resume(); };
    const hide = () => monitor.suspend();
    window.addEventListener('focus', focus); window.addEventListener('pageshow', focus); window.addEventListener('pagehide', hide);
    window.addEventListener('online', focus); window.addEventListener('offline', focus); document.addEventListener('visibilitychange', visibility);
    if (document.visibilityState === 'hidden') monitor.suspend();
    return () => {
      closed = true; monitor.dispose(); unsubscribe?.();
      window.removeEventListener('focus', focus); window.removeEventListener('pageshow', focus); window.removeEventListener('pagehide', hide);
      window.removeEventListener('online', focus); window.removeEventListener('offline', focus); document.removeEventListener('visibilitychange', visibility);
    };
  }, [recorder]);
  useEffect(() => {
    if (!native) return;
    let closed = false; let unsubscribe: (() => void) | undefined;
    listen<Task[]>('transfer-state', ({ payload }) => {
      if (closed) return;
      setTasks(payload);
      const finished = payload.filter(task => task.status === 'done' && task.video.savedPath && !completed.current.has(task.id));
      finished.forEach(task => completed.current.add(task.id));
      if (finished.length) savedRefresh.current++;
      if (finished.length) setSaved(previous => [...previous.filter(video => !finished.some(task => task.video.id === video.id)), ...finished.map(task => task.video)]);
    }).then(fn => { if (closed) fn(); else unsubscribe = fn; }).catch(error => setMessage({ text: errorText(error), error: true }));
    invoke<Task[]>('list_transfers').then(items => {
      if (closed) return;
      items.filter(task => task.status === 'done').forEach(task => completed.current.add(task.id));
      setTasks(items);
    }).catch(error => setMessage({ text: errorText(error), error: true }));
    return () => { closed = true; unsubscribe?.(); };
  }, []);
  useEffect(() => { setVisibleLimit(60); }, [filter, date, query]);
  const videos = useMemo(() => {
    const map=new Map<string,Video>();
    remote.forEach(video=>map.set(video.id,video));
    saved.forEach(video=>map.set(video.id,video));
    return [...map.values()];
  },[remote,saved]);
  const shown = useMemo(()=>visibleVideos(videos,filter,date,query),[videos,filter,date,query]);
  const dateGroups = useMemo(()=>groupVideosByDate(shown,visibleLimit),[shown,visibleLimit]);
  const notify = (text: string,error=false) => { setMessage({text,error}); if (native && error) void invoke('record_client_event', { category: 'frontend-error', message: text }).catch(() => {}); };
  async function queueAction(action: 'cancelAll' | 'clearHistory') {
    setQueueWorking(true);
    try {
      setTasks(native ? await invoke<Task[]>('control_transfers', { action }) : []);
    } catch (error) { notify(errorText(error), true); }
    finally { setQueueWorking(false); }
  }

  async function load(rec: Recorder) {
    const version=++connectionVersion.current;
    setLoading(true);clearMetadataCache();setMetadataVersion(value=>value+1);
    const result=await readVideoLibrary(rec.address,()=>version===connectionVersion.current);
    if(version!==connectionVersion.current) return;
    setRemote(previous=>[...previous.filter(video=>result.failed.includes(video.category)),...result.videos]);setLoading(false);
    if(result.warnings.length)notify(result.warnings.join('；'),true);
    else if(result.videos.length)notify('录像列表已更新');
    else notify('已连接设备，但设备未返回录像。请检查存储卡，或打开连接窗口复制诊断。',true);
  }
  async function enqueue(items:Video[]) {
    const eligible=items.filter(video=>!video.savedPath&&!tasks.some(task=>task.video.id===video.id&&['waiting','downloading'].includes(task.status)));
    if(!eligible.length){const text='所选录像已保存或已在传输队列中';notify(text);return text;}
    if(eligible.length>100){notify('每次最多选择 100 段录像',true);return '每次最多选择 100 段录像';}
    if(!recorder){notify('请先连接记录仪',true);return '请先连接记录仪';}
    try{setTasks(await invoke<Task[]>('enqueue_transfers',{address:recorder.address,videos:eligible}));setSelected(new Set());setSelecting(false);notify('已开始下载，可在传输任务中查看');return '已开始下载，可在传输任务中查看';}
    catch(error){notify(errorText(error),true);return errorText(error);}
  }
  async function cancel(task:Task) {await invoke('cancel_transfer',{taskId:task.id}).catch(error=>notify(errorText(error),true));}
  async function removeTask(task:Task) {
    try {setTasks(await invoke<Task[]>('delete_transfer',{taskId:task.id}));notify('任务已删除，已保存的录像保留');}
    catch(error) {notify(errorText(error),true);}
  }
  function retry(task:Task) {
    if(!recorder){notify('请先重新连接记录仪',true);setConnectOpen(true);return;}
    void invoke<Task[]>('retry_transfer',{taskId:task.id,address:recorder.address}).then(setTasks).catch(error=>notify(errorText(error),true));
  }
  function toggleSelection(id:string) {if(!selected.has(id)&&selected.size>=100){notify('每次最多选择 100 段录像');return;}setSelected(items=>{const next=new Set(items);if(next.has(id))next.delete(id);else next.add(id);return next;});}
  function switchPage(next:Page) {setPage(next);setSelected(new Set());setSelecting(false);}
  const connected = Boolean(recorder);

  return <div className="app-shell"><div className="glacier-backdrop" aria-hidden="true"/>
    <aside className="sidebar">
      <div className="brand"><button className="brand-name" aria-label="修改应用名称" title="点击修改名称" onClick={editAppName}>{displayName}<small>留住每一程</small></button></div>
      <div className="nav-label">工作空间</div>
      <nav aria-label="主导航">{nav.map(({key,label,short,icon:Icon})=><button key={key} className={'nav-item '+(page===key?'active':'')} aria-current={page===key?'page':undefined} onClick={()=>switchPage(key)}><Icon size={19}/><span className="nav-desktop-label">{label}</span><span className="nav-mobile-label">{short}</span>{key==='transfers'&&busy&&<span className="nav-count">{tasks.filter(t=>['waiting','downloading'].includes(t.status)).length}</span>}</button>)}</nav>
      <div className="sidebar-bottom"><div className="vehicle-mini"><span className="vehicle-icon"><CarFront size={20}/></span><div><strong>{homeVehicle}</strong><span><i className={connected?'status-dot online':'status-dot'}/>{recorder?'已连接记录仪':'等待连接'}</span></div></div><div className="sidebar-version">吉行影记 <span>v0.8.5</span></div></div>
    </aside>

    <div className="workspace" data-page={page}>
      <header className={'topbar topbar-'+page}><div className="mobile-page-heading"><button className="icon-button" aria-label="返回设备首页" onClick={()=>switchPage('device')}><ArrowLeft size={22}/></button><strong>{nav.find(item=>item.key===page)?.label}</strong></div><div className="breadcrumb">我的记录仪 <ChevronRight size={13}/><span>{nav.find(item=>item.key===page)?.label}</span></div><div className="topbar-actions">{page==='videos'&&<div className="glacier-library-actions"><button className="icon-button" aria-label="搜索与筛选" aria-expanded={filterToolsOpen} onClick={()=>setFilterToolsOpen(!filterToolsOpen)}><Search size={20}/></button><button className="icon-button" aria-label={selecting?'完成选择':'批量选择'} aria-pressed={selecting} onClick={()=>{setSelecting(!selecting);setSelected(new Set());}}><CheckCheck size={20}/></button></div>}<button className="icon-button help-button" aria-label="使用帮助" onClick={()=>switchPage('settings')}><CircleHelp size={20}/></button>{page==='videos'&&<button aria-label={recorder?'设备已连接':'连接记录仪'} className={'connection-button '+(connected?'connected':'')} disabled={busy} onClick={()=>setConnectOpen(true)}><Wifi size={17}/><span>{recorder?'设备已连接':'连接记录仪'}</span><ChevronRight size={13}/></button>}</div></header>
      <main className="main-content">
        {message&&<div className={'notice '+(message.error?'notice-error':'')} role={message.error?'alert':'status'}><span>{message.text}</span><button className="icon-button" aria-label="关闭提示" onClick={()=>setMessage(null)}><X size={15}/></button></div>}

        {page==='videos'&&<>
          <div className={'page-heading '+(filterToolsOpen?'is-tools-open':'')}><div><div className="eyebrow">YOUR JOURNEY, PRESERVED</div><h1>行车录像<span className="heading-dot">.</span></h1><p>{connected?'那些路过的风景，和值得保留的瞬间。':'连接记录仪，把路上的每一刻留在身边。'}</p></div><div className="heading-actions"><button className="secondary-button" disabled={!connected||loading} onClick={()=>recorder&&load(recorder)}><RefreshCw size={16} className={loading?'spinning':''}/><span>{loading?'读取中':'刷新录像'}</span></button>{videos.length>0&&<button className={'secondary-button '+(selecting?'selected-button':'')} onClick={()=>{setSelecting(!selecting);setSelected(new Set());}}><CheckCheck size={16}/><span>{selecting?'完成选择':'批量选择'}</span></button>}</div></div>
          <button type="button" className="glacier-recorder-strip" disabled={busy} onClick={()=>setConnectOpen(true)} aria-label="管理记录仪连接"><img src={appearance.deviceImage||'/images/glacier-device.webp'} alt=""/><div><strong>{homeVehicle}</strong><span className="subtle-text"><i className={connected?'status-dot online':'status-dot'}/>{connected?'已连接':'未连接'}</span></div><ChevronRight size={19}/></button>
          <div className="library-toolbar"><div className="filter-tabs" aria-label="录像分类">{([['all','全部录像'],['normal','普通录像'],['event','紧急录像'],['saved','已保存']] as const).map(([key,label])=><button key={key} aria-pressed={filter===key} className={filter===key?'active':''} onClick={()=>{setFilter(key);setSelected(new Set());}}>{label}{key==='saved'&&saved.length>0&&<span>{saved.length}</span>}</button>)}</div><div className={'toolbar-controls '+(filterToolsOpen?'is-open':'')}><label className="search-field"><Search size={15}/><input aria-label="搜索录像" placeholder="搜索录像" value={query} onChange={event=>setQuery(event.target.value)}/></label><input className="date-input" type="date" aria-label="按日期筛选" value={date} onChange={event=>setDate(event.target.value)}/>{date&&<button className="icon-button" aria-label="清除日期" onClick={()=>setDate('')}><X size={14}/></button>}<div className="view-toggle"><button className={view==='grid'?'active':''} aria-label="网格视图" aria-pressed={view==='grid'} onClick={()=>setView('grid')}><Grid2X2 size={16}/></button><button className={view==='list'?'active':''} aria-label="列表视图" aria-pressed={view==='list'} onClick={()=>setView('list')}><List size={17}/></button></div></div></div>
          {loading&&<div className="loading-state" role="status"><LoaderCircle className="spinning" size={20}/>正在读取记录仪录像，请稍候…</div>}
          {!loading&&shown.length>0&&<>
            {selecting&&<div className="library-selection-toolbar"><span>共 {shown.length} 段录像</span><button className="text-button" onClick={()=>setSelected(selected.size===Math.min(100,shown.length)?new Set():new Set(shown.slice(0,100).map(video=>video.id)))}>{selected.size===Math.min(100,shown.length)?'清空选择':shown.length>100?'选择前 100 段':'选择全部'}</button></div>}
            {dateGroups.map(group=><section className="recording-date-group" key={group.date} aria-label={group.date||'日期未知'} data-date={group.date}>
              <header className="date-heading"><h2>{group.date?<time dateTime={group.date}>{group.date}</time>:'日期未知'}<span>{group.count} 段录像</span></h2></header>
              <div className={'video-collection '+view}>{group.videos.map(video=><VideoCard key={video.id+':'+metadataVersion} video={video} address={recorder?.address} selecting={selecting} selected={selected.has(video.id)} onSelect={()=>toggleSelection(video.id)} onOpen={video=>openVideo(video)} onMenu={setActionVideo} onSave={item=>void enqueue([item])}/>)}</div>
            </section>)}
            {shown.length>visibleLimit&&<button className="secondary-button load-more-recordings" onClick={()=>setVisibleLimit(value=>value+60)}>加载更多录像（已显示 {Math.min(visibleLimit,shown.length)} / {shown.length}）</button>}
          </>}

          {!loading&&shown.length===0&&<div className="empty-state"><span className="empty-icon"><Film size={27}/></span><h2>{filter==='saved'?'还没有保存的录像':date||query?'没有符合条件的录像':connected?'还没有读取到录像':'录像将在这里等你'}</h2><p>{filter==='saved'?'把想要留住的片段，保存到相册。':date||query?'试试清除筛选条件，查看其他录像。':connected?'刷新列表，或检查车机记录仪是否正常工作。':'普通录像、紧急录像和已保存片段，都能轻松找到。'}</p>{(date||query)&&<button className="text-button" onClick={()=>{setDate('');setQuery('');}}>清除筛选条件</button>}</div>}
          {selected.size>0&&<div className="selection-bar"><span><Check size={16}/>已选 {selected.size} 段录像</span><div><button className="quiet-button" onClick={()=>setSelected(new Set())}>取消</button>{videos.some(video=>selected.has(video.id)&&video.savedPath)&&<><button className="quiet-button" onClick={()=>setBatchVideos(videos.filter(video=>selected.has(video.id)&&Boolean(video.savedPath)))}>导出与分享</button><button className="quiet-button danger-text" onClick={()=>setDeleteConfirm(true)}><Trash2 size={16}/>删除已保存</button></>}<button className="primary-button" onClick={()=>void enqueue(videos.filter(video=>selected.has(video.id)))}><Download size={16}/>保存录像</button></div></div>}
        </>}

        {page==='device'&&<DeviceHome vehicleName={homeVehicle} vehicleInfo={lastDevice?.vehicle||null} title={homeTitle} image={appearance.deviceImage} onTitle={()=>setAppearanceEditor('title')} onImage={()=>setAppearanceEditor('image')} onVehicle={()=>lastDevice?setAppearanceEditor('vehicle'):setConnectOpen(true)} recorder={recorder} checking={connectionChecking} connecting={connecting} onReconnect={reconnectFromPull} busy={busy} videoCount={videos.length} savedCount={saved.length} pendingCount={tasks.filter(task=>['waiting','downloading'].includes(task.status)).length} onConnect={()=>setConnectOpen(true)} onLive={()=>setLiveOpen(true)} onNavigate={switchPage}/>}

        {page==='transfers'&&<><div className="page-heading"><div><div className="eyebrow">KEEP WHAT MATTERS</div><h1>传输任务<span className="heading-dot">.</span></h1><p>为值得留住的瞬间，留一份原画质。</p></div><div className="heading-actions transfer-actions"><button className="secondary-button" disabled={!busy || queueWorking} onClick={()=>void queueAction('cancelAll')}>取消全部下载</button><button className="secondary-button" disabled={queueWorking || !tasks.some(task=>!['waiting','downloading'].includes(task.status))} onClick={()=>void queueAction('clearHistory')}>清理传输记录</button><span className="status-pill">{tasks.filter(task=>task.status==='done').length} 项已完成</span></div></div>{tasks.length===0?<div className="empty-state large"><span className="empty-icon"><ArrowDownToLine size={28}/></span><h2>暂时没有传输任务</h2><p>从行车录像中选择片段，保存到相册。</p><button className="primary-button" onClick={()=>switchPage('videos')}>去选择录像<ArrowRight size={16}/></button></div>:<div className="transfer-list">{tasks.map(task=><TransferCard key={task.id} task={task} onPause={()=>void cancel(task)} onResume={()=>retry(task)} onDelete={()=>void removeTask(task)} onOpenSaved={()=>{setFilter('saved');switchPage('videos');}}/>)}</div>}</>}

        {page==='settings'&&<SettingsPage busy={busy} recorder={recorder} savedCount={saved.length} onSaved={()=>{setFilter('saved');switchPage('videos');}} onConnect={()=>setConnectOpen(true)} onLive={()=>setLiveOpen(true)} onMessage={notify}/>}

      </main>
      <footer className="workspace-footer"><span><ShieldCheck size={13}/>你的录像，留在你的设备里</span><span>吉行影记 · {homeVehicle}</span></footer>
    </div>
    {appearanceEditor&&<AppearanceEditor kind={appearanceEditor} appearance={appearance} device={lastDevice} onClose={()=>setAppearanceEditor(null)} onSaved={next=>{setAppearance(next);notify('设备外观已保存');}} onVehicleDetails={()=>{setAppearanceEditor(null);setVehicleOpen(true);}} onAppName={()=>{setAppearanceEditor(null);editAppName();}}/>}
    {vehicleOpen&&lastDevice&&<VehicleDialog device={lastDevice} onClose={closeVehicle} onSaved={vehicleSaved}/>}
    {connectOpen&&<ConnectDialog profile={connectionProfile} onProfileChange={chooseProfile} busy={busy} connecting={connecting} recorder={recorder} onClose={()=>setConnectOpen(false)} onCancel={stopConnecting} onConnect={connectDevice} onDisconnect={()=>{
      if(busy)return;stopConnecting();localStorage.removeItem('galaxy-session');
      if(native)void releaseConnection();
      connectionVersion.current++;setRecorder(null);setRemote([]);setLoading(false);setConnectOpen(false);notify('已断开记录仪连接');
    }}/>}
    {currentVideo&&<Player key={currentVideo.id+':'+playerSequence} initialEditor={playerEdit} actionsOpen={Boolean(actionVideo)} onActions={setActionVideo} video={currentVideo} recorder={recorder} onClose={()=>setCurrentVideo(null)} onSave={item=>enqueue([item])} onSaved={video=>{changedVideo(video);setPlayerEdit(true);setCurrentVideo(video);}} onMessage={notify}/>}
    {liveOpen&&<Player recorder={recorder} live onClose={()=>setLiveOpen(false)} onSave={async()=>''} onSaved={()=>{}} onMessage={notify}/>}
    {actionVideo&&<VideoActions video={actionVideo} onClose={()=>setActionVideo(null)} onEdit={()=>{const video=actionVideo;setActionVideo(null);openVideo(video,true);}} onSave={()=>enqueue([actionVideo])} onChanged={changedVideo} onDeleted={()=>{const id=actionVideo.id;setSaved(items=>items.filter(video=>video.id!==id));setRemote(items=>items.map(video=>video.id===id?{...video,savedPath:undefined,storage:undefined}:video));setSelected(items=>{const next=new Set(items);next.delete(id);return next;});if(currentVideo?.id===id)setCurrentVideo(null);notify('录像文件已删除');}}/>}
    {nameOpen&&<Dialog label="修改应用名称" onClose={()=>{if(!nameWorking)setNameOpen(false);}}><form className="modal-content name-editor" onSubmit={event=>{event.preventDefault();void saveAppName();}}><h2>给应用起个名字</h2><p className="modal-description">修改软件内显示的名称，最多 32 个字符。</p><label className="form-field">应用名称<input aria-label="应用名称" autoFocus maxLength={32} value={nameDraft} disabled={nameWorking} onChange={event=>setNameDraft(event.target.value)}/></label>{nameError&&<p className="form-error" role="alert">{nameError}</p>}<div className="name-editor-actions"><button type="button" className="text-button" disabled={nameWorking} onClick={()=>setNameDraft(appName)}>恢复默认</button><button type="button" className="secondary-button" disabled={nameWorking} onClick={()=>setNameOpen(false)}>取消</button><button className="primary-button" disabled={nameWorking||!nameDraft.trim()}>保存名称</button></div></form></Dialog>}
    {batchVideos&&<BatchActions videos={batchVideos} onClose={()=>setBatchVideos(null)} onSaved={video=>setSaved(items=>[...items.filter(item=>item.id!==video.id),video])} onMessage={notify}/>}
    {deleteConfirm&&<Dialog label="删除已保存录像" onClose={()=>setDeleteConfirm(false)}><div className="modal-content"><h2>删除选中的录像文件？</h2><p className="modal-description">将删除选中的已保存文件，包括保存在相册或文件夹中的原文件。记录仪上的录像保留。</p><div className="player-action-row"><button className="secondary-button" onClick={()=>setDeleteConfirm(false)}>取消</button><button className="primary-button danger-button" onClick={async()=>{
      const ids=videos.filter(video=>selected.has(video.id)&&video.savedPath).map(video=>video.id);
      try{if(ids.length)await invoke('delete_saved',{ids});setSaved(items=>items.filter(video=>!selected.has(video.id)));setSelected(new Set());notify('已删除选中的录像文件');}
      catch(error){notify(errorText(error),true);}finally{setDeleteConfirm(false);if(native)void readSaved().then(setSaved);}
    }}><Trash2 size={16}/>确认删除</button></div></div></Dialog>}

  </div>;
}

export default App;
