import { ArrowDownUp, ChevronRight, Clapperboard, Settings, Video, Wifi } from 'lucide-react';
import type { Page, Recorder } from './types';

type Props = {
  recorder: Recorder | null;
  demo: boolean;
  busy: boolean;
  videoCount: number;
  savedCount: number;
  pendingCount: number;
  onConnect: () => void;
  onDemo: () => void;
  onNavigate: (page: Page) => void;
};

export default function DeviceHome({ recorder, demo, busy, videoCount, savedCount, pendingCount, onConnect, onDemo, onNavigate }: Props) {
  const connected = demo || Boolean(recorder);
  return <div className="glacier-home">
    <header className="glacier-home-heading">
      <h1>我的记录仪</h1>
      <p>记录旅途，守护每一段前行</p>
    </header>

    <section className="glacier-device" aria-label="星越 L 记录仪">
      <div className="tech-product-image glacier-product-stage">
        <div className="glacier-orbit orbit-outer" aria-hidden="true" />
        <div className="glacier-orbit orbit-inner" aria-hidden="true" />
        <img src="/images/glacier-device.png" alt="银白色记录仪概念展示" />
        <div className="glacier-product-wordmark" aria-hidden="true">GALAXY<br />DASHCAM<span /></div>
        <span className="glacier-concept-label">设备示意</span>
      </div>
      <div className="glacier-model"><h2>星越 L</h2><span>2026 款</span></div>
      <div className={'glacier-connection-state ' + (connected ? 'online' : '')}><i />{demo ? '演示设备' : connected ? '已连接记录仪' : '未连接'}</div>
      <div className="glacier-capabilities">Wi-Fi 直连<span>·</span>原画质保存</div>
      <button className="glacier-connect-button" disabled={busy} onClick={connected ? () => onNavigate('videos') : onConnect}>
        {connected ? <Clapperboard size={20} /> : <Wifi size={21} />}
        <span>{connected ? '查看行车录像' : '连接记录仪'}</span>
        <ChevronRight size={20} />
      </button>
    </section>

    <section className="glacier-shortcuts" aria-label="快捷操作">
      <button onClick={() => onNavigate('videos')}><span className="glacier-shortcut-icon"><Video size={22} /></span><strong>行车录像</strong><small>{connected ? `共 ${videoCount} 段录像` : '查看与管理'}</small></button>
      <button onClick={() => onNavigate('transfers')}><span className="glacier-shortcut-icon"><ArrowDownUp size={23} /></span><strong>传输任务</strong><small>{connected ? `正在传输 ${pendingCount}` : '传输与保存'}</small></button>
      <button onClick={() => onNavigate('settings')}><span className="glacier-shortcut-icon"><Settings size={22} /></span><strong>设备设置</strong><small>{connected && savedCount ? `已保存 ${savedCount} 段` : '记录仪参数'}</small></button>
    </section>
    <button className="glacier-demo-link" disabled={busy} onClick={connected ? onConnect : onDemo}>{connected ? '管理设备连接' : '不在车旁？先体验演示'}<ChevronRight size={12} /></button>
  </div>;
}
