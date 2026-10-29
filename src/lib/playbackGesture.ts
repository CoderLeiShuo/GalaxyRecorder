export type GestureMode = 'seek' | 'brightness' | 'volume';
export function gestureMode(dx:number,dy:number,startX:number,width:number,threshold=12):GestureMode|null {
  if(Math.max(Math.abs(dx),Math.abs(dy))<threshold)return null;
  return Math.abs(dx)>=Math.abs(dy)?'seek':startX<width/2?'brightness':'volume';
}
export function gestureSeek(start:number,dx:number,width:number,duration:number) {
  return Math.max(0,Math.min(duration,start+dx/Math.max(1,width)*Math.min(duration,120)));
}
export function gestureLevel(start:number,dy:number,height:number,minimum=0) {
  return Math.max(minimum,Math.min(1,start-dy/Math.max(1,height*.7)));
}
