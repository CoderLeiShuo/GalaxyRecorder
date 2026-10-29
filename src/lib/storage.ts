export function statusField(fields:Record<string,string>,...names:string[]) {
 for(const name of names){const match=Object.entries(fields).find(([key,value])=>value.trim()&&(key.toLowerCase()===name.toLowerCase()||key.toLowerCase().endsWith('.'+name.toLowerCase())));if(match)return match[1].trim();}
 return undefined;
}
export function storageBytes(value:string|undefined):number|null {
 const match=/^(\d+(?:\.\d+)?)\s*(B|K|M|G|T|KB|MB|GB|TB|KiB|MiB|GiB|TiB)?$/i.exec(value?.trim()||'');if(!match)return null;
 const unit=(match[2]||'B').toLowerCase(),power=unit==='b'?0:({k:1,m:2,g:3,t:4} as Record<string,number>)[unit[0]];
 const bytes=Number(match[1])*1024**power;return Number.isFinite(bytes)&&bytes<=Number.MAX_SAFE_INTEGER?Math.round(bytes):null;
}
export function storageSummary(fields:Record<string,string>){
 const total=storageBytes(statusField(fields,'TotalSize','LifeTimeTotal','TotalCapacity'));
 const free=storageBytes(statusField(fields,'FreeSize','RemainLifeTime','AvailableSize'));
 const state=statusField(fields,'Sd','SD0');
 return {total,free:free!=null&&(total==null||free<=total)?free:null,state:state?/^(0|ready|normal|ok)$/i.test(state)?'正常':/^(1|absent|removed|no.?card)$/i.test(state)?'未插入':/^(error|fail|fault)$/i.test(state)?'异常':state:null};
}
export function storageUsage(fields: Record<string, string>) {
 const summary=storageSummary(fields),total=summary.total,values=['usedNormalSpace','usedEventSpace','usedPhotoSpace'].map(name=>storageBytes(statusField(fields,name)));
 if(summary.state!=='正常'||total==null||total<=0||values.some(value=>value==null))return null;
 const used=values.reduce<number>((sum,value)=>sum+(value??0),0);if(used>total*1.01)return null;
 return {total,used:Math.min(total,used),free:summary.free??Math.max(0,total-used),values:values as number[]};
}
export function deviceVersions(fields:Record<string,string>,fallback:string){
 const firmware=statusField(fields,'SOCVer','FWversion');const pieces=firmware?.split(';');
 return {firmware:pieces&&pieces.length>=9?pieces[7]:firmware||fallback,controller:statusField(fields,'MCUVer','MCUversion','MCUVersion','ControllerVersion')};
}
