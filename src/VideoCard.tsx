import { useEffect, useRef, useState } from 'react';
import { Camera, Check, Download, Film, LoaderCircle, MoreVertical, Play, ShieldAlert } from 'lucide-react';
import type { Video, VideoMetadata } from './types';
import { formatDuration, formatSize } from './lib/recorder';
import { getMetadata } from './lib/media';

export default function VideoCard({ video, address, selecting, selected, onSelect, onOpen, onSave, onMenu }: { video: Video; address?: string; selecting: boolean; selected: boolean; onSelect: () => void; onOpen: (video: Video) => void; onSave: (video: Video) => void; onMenu: (video: Video) => void }) {
  const root = useRef<HTMLElement>(null); const [meta, setMeta] = useState<VideoMetadata | null>(null); const [loading, setLoading] = useState(false); const [coverError, setCoverError] = useState(false);
  useEffect(() => {
    let active = true; let requested = false;
    const load = () => {
      if (requested || !active) return; requested = true; setLoading(true);
      getMetadata(address, video).then(value => { if (active) setMeta(value); }).catch(() => { if (active) setMeta({ duration: null, thumbnail: null, warning: '暂时无法读取预览' }); }).finally(() => { if (active) setLoading(false); });
    };
    const observer = new IntersectionObserver(entries => { if (entries.some(entry => entry.isIntersecting)) { load(); observer.disconnect(); } }, { rootMargin: '180px' });
    if (root.current) observer.observe(root.current);
    return () => { active = false; observer.disconnect(); };
  }, [video.id, video.savedPath, address]);
  const item = { ...video, size: meta?.size || video.size, duration: meta?.duration ?? video.duration, thumbnail: meta?.thumbnail ?? video.thumbnail };
  const cover = item.thumbnail;
  return <article ref={root} className={'video-card ' + (selected ? 'is-selected' : '')} onClick={event=>{if((event.target as HTMLElement).closest('button,input,[role=checkbox]'))return;if(selecting)onSelect();else onOpen(item);}}>
    <button className={'video-image ' + (cover && !coverError ? '' : 'video-placeholder')} onClick={selecting ? onSelect : () => onOpen(item)} aria-label={(selecting ? '选择' : '播放') + ' ' + (video.time || video.name)}>
      {cover && !coverError ? <img className="recording-cover" src={cover} alt="录像封面" loading="lazy" onError={() => setCoverError(true)} /> : <span className="cover-fallback">{loading ? <LoaderCircle className="spinning" size={26} /> : <Film size={32} strokeWidth={1.2} />}<small>{loading ? '正在读取封面' : '暂无可用封面'}</small></span>}
      <span className="video-kind">{video.category === 'event' ? <ShieldAlert size={11} /> : <Camera size={11} />} {video.category === 'event' ? '紧急录像' : '普通录像'}</span>
      {selecting ? <span className={'video-checkbox ' + (selected ? 'checked' : '')} role="checkbox" aria-checked={selected}>{selected && <Check size={13} />}</span> : <span className="play-overlay"><Play size={20} fill="currentColor" /></span>}
      <span className="video-duration">{item.duration != null ? formatDuration(item.duration) : loading || !meta ? '读取时长中' : '未提供时长'}</span>
    </button>
    <div className="video-info"><div><button className="video-title" onClick={selecting ? onSelect : () => onOpen(item)}>{video.time || video.name}</button><span className={'video-category-inline ' + (video.category === 'event' ? 'event' : '')}>{video.category === 'event' ? '紧急' : '普通'}</span><p>{video.date || '日期待读取'}<span>·</span>{formatSize(item.size)}</p></div>
      <button className={'save-button ' + (video.savedPath ? 'saved' : '')} disabled={Boolean(video.savedPath)} onClick={() => onSave(item)} aria-label={video.savedPath ? '已保存' : '保存 ' + video.time + ' 的录像'}>{video.savedPath ? <Check size={17} /> : <Download size={17} />}<span>{video.savedPath ? '已存' : '保存'}</span></button>
      <button className="video-more-button" aria-label="录像文件操作" onClick={() => onMenu(item)}><MoreVertical size={15} /></button>
    </div>
  </article>;
}
