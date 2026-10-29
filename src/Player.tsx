import { useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { ArrowLeft, Check, Download, Expand, LoaderCircle, Minimize, Pause, Play, RotateCcw, MoreVertical, ShieldCheck, SkipBack, SkipForward, Volume2, VolumeX } from 'lucide-react';
import type { MaskRegion, MediaSource, Recorder, Video } from './types';
import Dialog from './Dialog';
import { android, ios, clampSeek, getMetadata, getSettings, nativeMedia, playerTime } from './lib/media';
import { clampTransform, identity, zoomAt, type Transform } from './lib/panZoom';
import { gestureMode, gestureSeek, gestureLevel, type GestureMode } from './lib/playbackGesture';
import { formatSize } from './lib/recorder';

type Props = { initialEditor?: boolean; actionsOpen?: boolean; onActions?: (video:Video)=>void; video?: Video; recorder: Recorder | null; live?: boolean; onClose: () => void; onSave: (video: Video) => Promise<string>; onSaved: (video: Video) => void; onMessage: (text: string, error?: boolean) => void };
type NativeStatus = { position: number; duration: number | null; playing: boolean; buffering: boolean; error: string | null; exportProgress: number; fullscreen?: boolean; action?: string | null; volume?: number;volumeSteps?:number };

export default function Player({ video, recorder, live = false, initialEditor = false, actionsOpen = false, onActions, onClose, onSave, onSaved, onMessage }: Props) {
  const nativeEngine = android;
  const stage = useRef<HTMLDivElement>(null); const element = useRef<HTMLVideoElement>(null);
  const [source, setSource] = useState<MediaSource | null>(null);
  const [position, setPosition] = useState(0); const [duration, setDuration] = useState(video?.duration || 0);
  const [playing, setPlaying] = useState(false); const [buffering, setBuffering] = useState(true); const [error, setError] = useState('');
  const [speedOpen,setSpeedOpen]=useState(false);
  const [volume, setVolume] = useState(nativeEngine?0:1);const [volumeStep,setVolumeStep]=useState(.05);const audibleVolume=useRef(.5); const [speed, setSpeed] = useState(1); const [transform,setTransform]=useState<Transform>(identity);const transformRef=useRef(identity);const pointers=useRef(new Map<number,{x:number;y:number}>()); const [expanded, setExpanded] = useState(false); const [reload, setReload] = useState(0);
  if(volume>0)audibleVolume.current=volume;
  const preferences = useRef({ volume, speed, fullscreen: 0 }); preferences.current = { volume, speed, fullscreen: expanded ? 1 : 0 };
  const [brightness,setBrightness]=useState(1);const [gestureHint,setGestureHint]=useState('');const [previewPosition,setPreviewPosition]=useState<number|null>(null);
  const gesture=useRef<{x:number;y:number;position:number;volume:number;brightness:number;mode:GestureMode|null;target:number}|null>(null);const hintTimer=useRef<ReturnType<typeof setTimeout>|undefined>(undefined);
  const [fileSize, setFileSize] = useState(video?.size || 0);
  const [toast, setToast] = useState(''); const [saving, setSaving] = useState(false);
  const [controlsVisible, setControlsVisible] = useState(true); const hideTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const [poster, setPoster] = useState(video?.thumbnail || ''); const [editing] = useState(initialEditor);
  const [trim, setTrim] = useState(false); const [start, setStart] = useState(0); const [end, setEnd] = useState(video?.duration || 0); const [mask, setMask] = useState(false);
  const [region, setRegion] = useState<MaskRegion>({ x: 0.62, y: 0, width: 0.38, height: 0.09 });
  const [exporting, setExporting] = useState(false); const [exportProgress, setExportProgress] = useState(0); const exportId = useRef('');
  const nativeReady=useRef(false);const gestureMoved=useRef(false);
  const actionHandler=useRef<()=>Promise<void>>(async()=>{});
  const close = () => { if (!exporting) onClose(); };
  const bounds = useCallback(() => {
    const rect = stage.current?.getBoundingClientRect();
    const dialog = stage.current?.closest('dialog')?.getBoundingClientRect();
    return rect ? { left: rect.left, top: rect.top, width: rect.width, height: rect.height, scale: window.devicePixelRatio,
      clipLeft: Math.max(0, -rect.left, (dialog?.left ?? 0) - rect.left), clipTop: Math.max(0, -rect.top, (dialog?.top ?? 0) - rect.top),
      clipRight: Math.min(rect.width, innerWidth - rect.left, (dialog?.right ?? innerWidth) - rect.left), clipBottom: Math.min(rect.height, innerHeight - rect.top, (dialog?.bottom ?? innerHeight) - rect.top) } : {};
  }, []);

  useEffect(() => { void getSettings().then(value => { setMask(value.maskByDefault); }); }, []);
  useEffect(() => {
    if (!ios) return;
    void nativeMedia<{volume:number}>('control', {action:'awake',value:1}).then(state=>setVolume(state.volume)).catch(()=>{});
    const poll=setInterval(()=>void nativeMedia<{volume:number;exportProgress:number}>('status').then(state=>{setVolume(state.volume);setExportProgress(state.exportProgress);}).catch(()=>{}),500);
    return ()=>{clearInterval(poll);void nativeMedia('control',{action:'awake',value:0}).catch(()=>{});};
  }, []);
  useEffect(() => {
    let current = true; nativeReady.current=false; setError(''); setBuffering(true); setSource(null);
    const open = async () => {
      const result = await invoke<MediaSource>(live ? 'live_media' : 'open_media', live ? { address: recorder?.address } : { address: recorder?.address ?? null, video });
      if (!current) return; setSource(result); if (result.size) setFileSize(result.size);
      void invoke('record_client_event', { category: 'playback', message: live ? '打开实时画面' : video?.savedPath ? '打开离线回看' : '打开在线回看' }).catch(() => {});
      if (nativeEngine) {
        const audio=await nativeMedia<{volume?:number;volumeSteps?:number}>('open', { source: result.nativeUrl, keepScreenOn: true, ...bounds() });
        if (!current) return;
        if(typeof audio.volume==='number'&&Number.isFinite(audio.volume))setVolume(audio.volume);if(audio.volumeSteps)setVolumeStep(1/audio.volumeSteps);
        for (const [action, value] of Object.entries(preferences.current).filter(([action])=>action!=='volume')) await nativeMedia('control', { action, value });
        nativeReady.current=true;
      }
      if (video) void getMetadata(recorder?.address, video).then(meta => {
        if (!current) return;
        if (meta.duration) { setDuration(meta.duration); setEnd(previous => previous || meta.duration!); }
        if (meta.thumbnail) setPoster(meta.thumbnail);
        if (meta.size) setFileSize(meta.size);
      }).catch(() => {});
    };
    void open().catch(error => { if (current) { setError(String(error)); setBuffering(false); } });
    return () => { current = false; if (nativeEngine) void nativeMedia('close').catch(() => {}); };
  }, [video?.id, video?.savedPath, recorder?.address, live, nativeEngine, reload, bounds]);

  useEffect(() => {
    if (!nativeEngine) return;
    let current = true; let timer: ReturnType<typeof setTimeout>; let frame = 0;
    const updateBounds = () => { cancelAnimationFrame(frame); frame = requestAnimationFrame(() => { void nativeMedia('bounds', bounds()).catch(() => {}); }); };
    const observer = new ResizeObserver(updateBounds); if (stage.current) observer.observe(stage.current);
    window.addEventListener('resize', updateBounds); window.addEventListener('scroll', updateBounds, true);
    const tick = async () => {
      if(!nativeReady.current){if(current)timer=setTimeout(tick,100);return;}
      try {
        const state = await nativeMedia<NativeStatus>('status'); if (!current) return;
        if(typeof state.volume==='number'&&Number.isFinite(state.volume))setVolume(state.volume);if(state.volumeSteps)setVolumeStep(1/state.volumeSteps);
        setPosition(state.position); setPlaying(state.playing); setBuffering(state.buffering); setExportProgress(state.exportProgress);
        if (state.fullscreen !== undefined) setExpanded(state.fullscreen);
        if (state.duration) { setDuration(state.duration); setEnd(previous => previous || state.duration!); }
        if (state.error) setError(state.error);
        if (state.action==='menu') void actionHandler.current();
        if (state.action==='retry')setReload(value=>value+1);
      } catch { /* Opening/closing a native surface can briefly interrupt status reads. */ }
      if (current) timer = setTimeout(tick, 300);
    }; void tick();
    return () => { current = false; clearTimeout(timer); cancelAnimationFrame(frame); observer.disconnect(); window.removeEventListener('resize', updateBounds); window.removeEventListener('scroll', updateBounds, true); };
  }, [nativeEngine, bounds]);

  function control(action: string, value?: number) {
    if (nativeEngine) { void nativeMedia('control', { action, value }).catch(error => setError(String(error))); return; }
    if (ios && action === 'volume') { void nativeMedia('control',{action,value}).catch(error=>setError(String(error))); return; }
    const player = element.current; if (!player) return;
    if (action === 'play') void player.play().catch(error => setError(String(error)));
    else if (action === 'pause') player.pause();
    else if (action === 'seek') player.currentTime = clampSeek(value!, duration);
    else if (action === 'volume') player.volume = value!;
    else if (action === 'speed') player.playbackRate = value!;
  }
  const seek = (value: number) => { const time = clampSeek(value, duration); setPosition(time); control('seek', time); };
  useEffect(() => {
    const key = (event: KeyboardEvent) => {
      if ((event.target as HTMLElement).matches('input,select,textarea,button')) return;
      if (event.code === 'Space') { event.preventDefault(); control(playing ? 'pause' : 'play'); }
      else if (event.code === 'ArrowLeft') { event.preventDefault(); seek(position - 5); }
      else if (event.code === 'ArrowRight') { event.preventDefault(); seek(position + 5); }
    }; window.addEventListener('keydown', key); return () => window.removeEventListener('keydown', key);
  }, [playing, position, duration, nativeEngine]);

  useEffect(() => {
    if (!toast) return; const timer = setTimeout(() => setToast(''), 4500); return () => clearTimeout(timer);
  }, [toast]);
  useEffect(() => {
    const changed = () => { if (!nativeEngine) setExpanded(document.fullscreenElement === stage.current); };
    document.addEventListener('fullscreenchange', changed);
    return () => { document.removeEventListener('fullscreenchange', changed); clearTimeout(hideTimer.current); clearTimeout(hintTimer.current); };
  }, [nativeEngine]);
  function toggleControls(){clearTimeout(hideTimer.current);setControlsVisible(visible=>!visible);}
  function revealControls() {
    setControlsVisible(true); clearTimeout(hideTimer.current);
    hideTimer.current = setTimeout(() => setControlsVisible(false), 3000);
  }
  useEffect(()=>{
    if(!expanded){transformRef.current=identity;setTransform(identity);pointers.current.clear();gesture.current=null;setGestureHint('');setPreviewPosition(null);setBrightness(1);}
  },[expanded]);
  useEffect(()=>{
    if(nativeEngine && source)void nativeMedia('control',{action:'visible',value:actionsOpen?0:1}).catch(()=>{});
  },[actionsOpen,nativeEngine,source]);
  function viewport(){const rect=stage.current!.getBoundingClientRect();const player=element.current;return {width:rect.width,height:rect.height,aspect:player?.videoWidth&&player?.videoHeight?player.videoWidth/player.videoHeight:16/9};}
  function applyTransform(next:Transform){transformRef.current=next;setTransform(next);}
  function pointerDown(event:React.PointerEvent<HTMLDivElement>){
    if(!expanded||nativeEngine||(event.target as HTMLElement).closest('button,input,select'))return;
    if(!pointers.current.size){gestureMoved.current=false;const rect=event.currentTarget.getBoundingClientRect();gesture.current={x:event.clientX-rect.left,y:event.clientY-rect.top,position:element.current?.currentTime??position,volume,brightness,mode:null,target:position};}else{gestureMoved.current=true;gesture.current=null;setPreviewPosition(null);setGestureHint('');}
    event.currentTarget.setPointerCapture(event.pointerId);pointers.current.set(event.pointerId,{x:event.clientX,y:event.clientY});
  }
  function pointerMove(event:React.PointerEvent<HTMLDivElement>){
    const previous=pointers.current.get(event.pointerId);if(!previous)return;
    if(Math.hypot(event.clientX-previous.x,event.clientY-previous.y)>6)gestureMoved.current=true;
    const before=[...pointers.current.values()];pointers.current.set(event.pointerId,{x:event.clientX,y:event.clientY});const after=[...pointers.current.values()];
    if(after.length===2){
      const distance=(points:{x:number;y:number}[])=>Math.hypot(points[0].x-points[1].x,points[0].y-points[1].y);
      const oldDistance=distance(before);if(oldDistance<2)return;
      const rect=stage.current!.getBoundingClientRect();
      const center={x:(after[0].x+after[1].x)/2-rect.left-rect.width/2,y:(after[0].y+after[1].y)/2-rect.top-rect.height/2};
      applyTransform(zoomAt(transformRef.current,distance(after)/oldDistance,center,viewport()));
    }else if(after.length===1&&transformRef.current.zoom>1){gesture.current=null;applyTransform(clampTransform({...transformRef.current,x:transformRef.current.x+event.clientX-previous.x,y:transformRef.current.y+event.clientY-previous.y},viewport()));}
    else if(after.length===1&&gesture.current){
      const g=gesture.current,rect=event.currentTarget.getBoundingClientRect(),dx=event.clientX-rect.left-g.x,dy=event.clientY-rect.top-g.y;
      g.mode??=gestureMode(dx,dy,g.x,rect.width);clearTimeout(hintTimer.current);
      if(g.mode==='seek'&&!live&&duration){g.target=gestureSeek(g.position,dx,rect.width,duration);setPreviewPosition(g.target);setGestureHint(`进度 ${playerTime(g.target)} / ${playerTime(duration)}`);}
      if(g.mode==='brightness'){const value=gestureLevel(g.brightness,dy,rect.height,.05);setBrightness(value);if(ios)void nativeMedia('control',{action:'brightness',value}).catch(()=>{});setGestureHint(`亮度 ${Math.round(value*100)}%`);}
      if(g.mode==='volume'){const value=gestureLevel(g.volume,dy,rect.height);setVolume(value);control('volume',value);setGestureHint(`音量 ${Math.round(value*100)}%`);}
    }
  }
  function pointerEnd(event:React.PointerEvent<HTMLDivElement>){
    if(!pointers.current.has(event.pointerId))return;
    const didGesture=gestureMoved.current||Boolean(gesture.current?.mode)||pointers.current.size>1;
    if(event.type==='pointerup'&&gesture.current?.mode==='seek'&&!live&&duration)seek(gesture.current.target);
    gesture.current=null;setPreviewPosition(null);pointers.current.delete(event.pointerId);
    hintTimer.current=setTimeout(()=>setGestureHint(''),800);if(didGesture)revealControls();else if(event.type==='pointerup')toggleControls();
    if(event.currentTarget.hasPointerCapture(event.pointerId))event.currentTarget.releasePointerCapture(event.pointerId);
  }
  async function showActions(){
    if(!video||!onActions)return;control('pause');
    if(nativeEngine){await nativeMedia('control',{action:'fullscreen',value:0});await nativeMedia('control',{action:'visible',value:0});}
    else if(document.fullscreenElement)await document.exitFullscreen();
    setExpanded(false);onActions(video);
  }
  actionHandler.current=showActions;
  async function toggleFullscreen() {
    if (nativeEngine) { control('fullscreen', expanded ? 0 : 1); return; }
    if (expanded) {
      if (document.fullscreenElement) await document.exitFullscreen();
      setExpanded(false);
    } else {
      setExpanded(true); revealControls();
      try { await stage.current?.requestFullscreen?.(); } catch { /* Full-viewport fallback for WebViews without Fullscreen API. */ }
    }
  }
  async function saveOriginal() {
    if (!video || saving) return; setSaving(true);
    try { setToast(await onSave({ ...video, size: fileSize, duration: duration || video.duration, thumbnail: poster || video.thumbnail })); }
    catch (error) { setToast(String(error)); }
    finally { setSaving(false); }
  }

  async function exportClip() {
    if (!video) { onMessage('请连接真实录像后导出片段'); return; }
    if (trim && (!duration || start < 0 || end <= start || end > duration + 0.1)) { onMessage('请选择录像范围内有效的起止时间', true); return; }
    control('pause'); setExporting(true); exportId.current = crypto.randomUUID();
    try {
      const result = await invoke<Video>('export_video', { address: recorder?.address ?? null, video: { ...video, size:fileSize, duration: duration || null }, start: trim ? start : 0, end: trim ? end : null, mask: mask ? region : null, jobId: exportId.current });
      setExporting(false); onSaved(result); onMessage(result.exportError || '片段已保存到系统相册', Boolean(result.exportError));
    } catch (error) { setExporting(false); onMessage(String(error), true); if (nativeEngine) setReload(value => value + 1); }
  }
  function updateRegion(key: keyof MaskRegion, value: number) {
    setRegion(previous => { const next = { ...previous, [key]: value }; next.width = Math.min(next.width, 1 - next.x); next.height = Math.min(next.height, 1 - next.y); return next; });
  }

  return <Dialog label={live ? '实时画面' : '录像播放器'} className={'custom-player-dialog ' + (expanded ? 'player-expanded' : '')} onClose={() => { if (expanded) void toggleFullscreen(); else close(); }}>
    <div className="custom-player">
      <header className="custom-player-heading"><button className="icon-button" aria-label="关闭播放器" disabled={exporting} onClick={close}><ArrowLeft size={21} /></button><div><strong>{live ? '实时画面' : video?.time || video?.name}</strong><span>{live ? (recorder?.vehicleInfo?.name||'记录仪')+' · 实时影像' : (video?.date || '行车录像') + ' · ' + (video?.savedPath ? '离线回看' : '在线回看')}</span></div><span className="player-source-badge">{live ? 'LIVE' : '原画质'}</span>{video&&onActions&&<button className="icon-button" aria-label="录像文件操作" disabled={exporting} onClick={()=>void showActions()}><MoreVertical size={19}/></button>}</header>
      <div ref={stage} className="custom-player-stage" style={{touchAction:expanded?'none':'auto'}} onPointerMove={pointerMove} onPointerDown={pointerDown} onPointerUp={pointerEnd} onPointerCancel={pointerEnd} onDoubleClick={()=>{if(expanded)applyTransform(identity);else control(playing?'pause':'play');}}>
        {!nativeEngine && source && (live ? <img className="live-stream" src={source.url} alt="记录仪实时画面" onLoad={() => setBuffering(false)} onError={() => { setError('实时画面连接失败，请确认电脑可运行 FFmpeg'); setBuffering(false); }} /> : <video ref={element} src={source.url} poster={poster || undefined} controls={false} autoPlay playsInline crossOrigin="anonymous" style={{ transform: `translate(${transform.x}px, ${transform.y}px) scale(${transform.zoom})`, filter: ios ? undefined : `brightness(${brightness})` }}
          onLoadedMetadata={event => { const player = event.currentTarget; player.volume = ios ? 1 : preferences.current.volume; player.playbackRate = preferences.current.speed; const value = player.duration; if (Number.isFinite(value)) { setDuration(value); setEnd(previous => previous || value); } }}
          onTimeUpdate={event => setPosition(event.currentTarget.currentTime)} onPlay={() => setPlaying(true)} onPause={() => setPlaying(false)} onEnded={() => setPlaying(false)} onWaiting={() => setBuffering(true)} onCanPlay={() => setBuffering(false)} onPlaying={() => setBuffering(false)} onError={() => { setError('录像解码或网络读取失败，请重试，或保存后回看'); setBuffering(false); }} />)}
        {!nativeEngine&&expanded&&error&&<div className="fullscreen-error" role="alert"><p>{error}</p><button onClick={()=>setReload(value=>value+1)}>重试播放</button></div>}
        {!nativeEngine && buffering && !error && <div className="player-stage-overlay"><LoaderCircle className="spinning" size={30} /><span>正在连接画面</span></div>}
        {!nativeEngine && !expanded && !playing && !buffering && !error && !live && <button className="player-center-play" aria-label="播放录像" onClick={() => control('play')}><Play size={32} fill="currentColor" /></button>}
        {expanded&&!nativeEngine&&gestureHint&&<div className="player-gesture-hint" role="status">{gestureHint}</div>}
        {expanded && !nativeEngine && <div className={'fullscreen-overlay-controls '+(controlsVisible?'visible':'')} onClick={event=>event.stopPropagation()} onDoubleClick={event=>event.stopPropagation()}>
          <button aria-label={playing?'暂停':'播放'} onClick={()=>{control(playing?'pause':'play');revealControls();}}>{playing?<Pause size={22}/>:<Play size={22}/>}</button>
          <div className="fullscreen-timeline"><span>{playerTime(previewPosition??position)}</span><input aria-label="全屏播放进度" type="range" min="0" max={duration||1} step="0.1" disabled={live||!duration} value={clampSeek(previewPosition??position,duration)} onChange={event=>{seek(Number(event.target.value));revealControls();}}/><span>{playerTime(duration)}</span></div>
          {video&&onActions&&<button aria-label="录像文件操作" onClick={()=>void showActions()}><MoreVertical size={21}/></button>}
          <button aria-label="退出全屏" onClick={()=>void toggleFullscreen()}><Minimize size={22}/></button>
        </div>}

      </div>
      {toast && <div className="player-toast" role="status">{toast}</div>}
      <div className="player-status-line" role="status">{error ? <><span className="player-error-text">{error}</span><button className="text-button" onClick={() => setReload(value => value + 1)}><RotateCcw size={14} />重试连接</button></> : buffering ? <><LoaderCircle className="spinning" size={13} />正在缓冲</> : <><span className="player-ready-dot" />{live ? '实时影像' : playing ? '正在播放' : '已暂停'}<span>{video ? fileSize ? formatSize(fileSize) : '正在读取文件大小' : 'TCP 视频流'}</span></>}</div>
      {!live && <div className="player-timeline"><span>{playerTime(position)}</span><input aria-label="播放进度" type="range" min="0" max={duration || 1} step="0.1" disabled={!duration || exporting} value={clampSeek(position, duration)} onChange={event => seek(Number(event.target.value))} style={{ '--played': `${duration ? position / duration * 100 : 0}%` } as React.CSSProperties} /><span>{duration ? playerTime(duration) : '--:--'}</span></div>}
      <div className="player-controls">
        <div className="player-preferences"><button className="icon-button" aria-label={volume ? '静音' : '取消静音'} onClick={() => { const next = volume ? 0 : audibleVolume.current; setVolume(next); control('volume', next); }}>{volume ? <Volume2 size={18} /> : <VolumeX size={18} />}</button><input className="volume-slider" aria-label="音量" type="range" min="0" max="1" step={volumeStep} value={volume} onChange={event => { const value = Number(event.target.value); setVolume(value); control('volume', value); }} />
          {!live&&<button className="speed-toggle" aria-label="播放速度" aria-expanded={speedOpen} disabled={exporting} onClick={()=>setSpeedOpen(!speedOpen)}>{speed}×</button>}
        </div>
        <div className="player-transport">{!live && <button className="icon-button" aria-label="后退 5 秒" disabled={exporting} onClick={() => seek(position - 5)}><SkipBack size={18} /></button>}
          <button className="player-play-button" aria-label={playing ? '暂停' : '播放'} disabled={!source || exporting || (live && !nativeEngine)} onClick={() => control(playing ? 'pause' : 'play')}>{playing ? <Pause size={23} fill="currentColor" /> : <Play size={23} fill="currentColor" />}</button>
          {!live && <button className="icon-button" aria-label="快进 5 秒" disabled={exporting} onClick={() => seek(position + 5)}><SkipForward size={18} /></button>}
        </div>
        <div className="player-view-controls">

          <button className="icon-button" aria-label={expanded ? '退出全屏' : '全屏'} disabled={exporting} onClick={() => void toggleFullscreen()}>{expanded ? <Minimize size={19} /> : <Expand size={19} />}</button>
        </div>
      </div>
      {speedOpen&&!live&&<div className="player-speed-options" role="group" aria-label="选择播放速度">{[0.5,1,1.25,1.5,2,3,4].map(value=><button key={value} aria-pressed={speed===value} disabled={exporting} onClick={()=>{setSpeed(value);control('speed',value);}}>{value}×</button>)}</div>}
      <p className="player-gesture-guide">全屏可滑动调进度，左侧调亮度，右侧调音量；双指缩放后可拖动画面</p>
      {video && !video.savedPath && <div className="player-action-row"><button className="primary-button" disabled={saving} onClick={() => void saveOriginal()}><Download size={16} />保存到相册</button></div>}
      {editing && video && <section className="clip-editor" aria-label="片段与车架号打码">
        <div className="editor-title"><h3>导出想要保留的画面</h3><ShieldCheck size={18} /></div>
        <label className="toggle-row"><span>只导出选中片段</span><input type="checkbox" checked={trim} disabled={exporting || !duration} onChange={event => setTrim(event.target.checked)} /></label>
        {trim && <div className="clip-range-fields"><label>开始（秒）<input type="number" min="0" max={end} step="0.1" value={start} disabled={exporting} onChange={event => setStart(Number(event.target.value))} /></label><button className="text-button" onClick={() => setStart(Math.min(position, end - 0.1))}>设为当前</button><label>结束（秒）<input type="number" min={start} max={duration} step="0.1" value={end} disabled={exporting} onChange={event => setEnd(Number(event.target.value))} /></label><button className="text-button" onClick={() => setEnd(Math.max(start + 0.1, position))}>设为当前</button></div>}
        <label className="toggle-row"><span>遮挡车架号水印<small>请按实际画面调整遮挡位置</small></span><input type="checkbox" checked={mask} disabled={exporting} onChange={event => setMask(event.target.checked)} /></label>
        {mask && <><div className="mask-preview">{poster && <img src={poster} alt="遮挡区域预览" />}<span style={{ left: region.x * 100 + '%', top: region.y * 100 + '%', width: region.width * 100 + '%', height: region.height * 100 + '%' }} /></div><div className="mask-sliders">{([['x', '左右位置'], ['y', '上下位置'], ['width', '遮挡宽度'], ['height', '遮挡高度']] as const).map(([key, label]) => <label key={key}>{label}<input type="range" min={key === 'width' || key === 'height' ? 0.005 : 0} max={key === 'width' ? 1 - region.x : key === 'height' ? 1 - region.y : 0.98} step="0.001" value={region[key]} disabled={exporting} onChange={event => updateRegion(key, Number(event.target.value))} /><span>{Math.round(region[key] * 100)}%</span></label>)}</div></>}
        <p className="editor-note">导出会生成新的 MP4 文件，原片保留。打码后会重新编码，请确认遮挡范围覆盖完整水印。</p>
        {exporting ? <div className="export-status" role="status"><LoaderCircle className="spinning" size={17} />正在导出{exportProgress ? ` ${exportProgress}%` : '…'}<button className="text-button" onClick={() => void invoke('cancel_export', { jobId: exportId.current })}>取消导出</button></div> : <button className="primary-button full-width" disabled={!trim && !mask} onClick={() => void exportClip()}><Check size={17} />{mask ? '导出打码录像' : '导出片段'}</button>}
      </section>}
      {live && <div className="live-controls">{([['record', '循环录像'], ['stop', '暂停录制'], ['capture', '拍照'], ['event', '紧急录像']] as const).map(([command, label]) => <button key={command} className="secondary-button" onClick={() => void invoke('recorder_command', { address: recorder?.address, command, value: null }).then(() => onMessage(label + '指令已发送，请在车机确认')).catch(error => onMessage(String(error), true))}>{label}</button>)}<p>拍照与紧急录像保存在记录仪存储卡。</p></div>}
    </div>
  </Dialog>;
}
