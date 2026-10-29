import {describe,it,expect} from 'vitest';
import {clampTransform,identity,zoomAt} from './panZoom';
describe('fullscreen gestures',()=>{
 it('anchors pinch zoom at the fingers rather than the center',()=>{expect(zoomAt(identity,2,{x:50,y:30},{width:400,height:200,aspect:2})).toEqual({zoom:2,x:-50,y:-30});});
 it('bounds dragging at picture edges and preserves letterboxing',()=>{expect(clampTransform({zoom:2,x:900,y:-900},{width:400,height:800,aspect:2})).toEqual({zoom:2,x:200,y:0});});
 it('limits scale and resets panning when returning to original size',()=>{expect(clampTransform({zoom:1,x:500,y:300},{width:400,height:200,aspect:2})).toEqual(identity);expect(zoomAt(identity,99,{x:0,y:0},{width:400,height:200,aspect:2}).zoom).toBe(5);});
});
