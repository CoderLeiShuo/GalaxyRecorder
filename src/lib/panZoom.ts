export type Viewport={width:number;height:number;aspect:number};
export type Transform={zoom:number;x:number;y:number};
export const identity:Transform={zoom:1,x:0,y:0};
export function clampTransform(next:Transform,view:Viewport):Transform {
 const zoom=Math.max(1,Math.min(5,Number.isFinite(next.zoom)?next.zoom:1));
 const width=Math.min(view.width,view.height*view.aspect)*zoom;const height=width/view.aspect;
 const dx=Math.max(0,(width-view.width)/2),dy=Math.max(0,(height-view.height)/2);
 return {zoom,x:dx?Math.max(-dx,Math.min(dx,next.x)):0,y:dy?Math.max(-dy,Math.min(dy,next.y)):0};
}
export function zoomAt(current:Transform,ratio:number,point:{x:number;y:number},view:Viewport):Transform {
 const zoom=Math.max(1,Math.min(5,current.zoom*ratio));const scale=zoom/current.zoom;
 return clampTransform({zoom,x:point.x-(point.x-current.x)*scale,y:point.y-(point.y-current.y)*scale},view);
}
