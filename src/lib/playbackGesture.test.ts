import {describe,it,expect} from 'vitest';
import {gestureMode,gestureSeek,gestureLevel} from './playbackGesture';
describe('fullscreen playback gestures',()=>{
  it('ignores jitter and locks horizontal seeking on either half',()=>{expect(gestureMode(8,9,10,400)).toBeNull();expect(gestureMode(-40,15,350,400)).toBe('seek');});
  it('routes vertical movement by initial touch side',()=>{expect(gestureMode(3,-40,30,400)).toBe('brightness');expect(gestureMode(3,40,300,400)).toBe('volume');});
  it('seeks relative to gesture start with a two minute maximum span',()=>{expect(gestureSeek(300,200,400,3600)).toBe(360);expect(gestureSeek(5,-400,400,12)).toBe(0);expect(gestureSeek(5,400,400,12)).toBe(12);});
  it('bounds volume and prevents a fully black picture',()=>{expect(gestureLevel(.5,-140,400)).toBe(1);expect(gestureLevel(.5,400,400)).toBe(0);expect(gestureLevel(.5,400,400,.05)).toBe(.05);});
});
