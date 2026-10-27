import { describe, expect, it } from 'vitest';
import { demoVideos, formatDuration, validateAddress, videoSource, visibleVideos } from './recorder';

describe('recorder connection boundaries',()=>{
  it('normalizes private addresses without accepting web paths',()=>{
    expect(validateAddress(' http://192.168.43.123:8080/ ')).toBe('192.168.43.123:8080');
    expect(validateAddress('10.0.0.1:80')).toBe('10.0.0.1');
    for(const value of ['192.168.1.1/a','8.8.8.8','127.0.0.1','192.168.1.256','192.168.1.01','192.168.1.1:0','192.168.1.1:65536','192.168.1.1?x=1'])expect(()=>validateAddress(value),value).toThrow();
  });
});
describe('library filtering',()=>{
  it('combines date, category, query and saved status',()=>{
    const items=[{...demoVideos[0],savedPath:'demo'},demoVideos[2]];
    expect(visibleVideos(items,'saved','','')).toHaveLength(1);
    expect(visibleVideos(items,'event','2026-10-27','082106')).toHaveLength(1);
    expect(visibleVideos(items,'event','2026-10-26','')).toHaveLength(0);
    expect(visibleVideos(items,'all','','')[0].time).toBe('08:26:40');
  });
  it('does not try to play a fictitious demo file',()=>{
    expect(videoSource({...demoVideos[0],savedPath:'demo'})).toBeUndefined();
    expect(formatDuration(null)).toBe('时长待读取');
  });
});
