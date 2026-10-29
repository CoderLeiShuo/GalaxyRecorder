import type {Video} from '../types';
export const recordingFixtures:Video[]=[
 ['08:26:40','normal',186],['08:23:40','normal',184],['08:21:06','event',32],
].map(([time,category,size],i)=>({id:'fixture-'+i,name:'20261027_'+String(time).replaceAll(':','')+'.mp4',path:'/fixture/'+i+'.mp4',category:category as 'normal'|'event',date:'2026-10-27',time:String(time),size:Number(size)*1024**2,duration:12}));
