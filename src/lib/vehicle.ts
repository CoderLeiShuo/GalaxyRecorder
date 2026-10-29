import type {DeviceMemory,ConnectionProfile} from '../types';
export const connectionProfiles:{value:ConnectionProfile;label:string;description:string;models:string[]}[]=[
 {value:'AUTO',label:'自动连接',description:'自动尝试可用方案',models:[]},
 {value:'A7',label:'A7 方案',description:'连接车机热点，开启原车记录仪',models:['银河A7','星耀7','M7','TT','2026款星舰7','2026款星瑞','2026款博越','2026款星越L','星愿']},
 {value:'E5',label:'E5 方案',description:'支持搜索并连接记录仪 Wi-Fi',models:['银河E5','星耀8','2025款星舰7','2025款星瑞']},
 {value:'L6',label:'L6 / L7 方案',description:'可使用吉行小助手的车型',models:['银河L6','银河L7','2023/2024款领克07','2023/2024款领克08']},
 {value:'GENERIC1',label:'通用模式一',description:'E5 兼容协议',models:[]},
 {value:'GENERIC2',label:'通用模式二',description:'A7 兼容协议',models:[]},
 {value:'GENERIC3',label:'通用模式三',description:'L6 / L7 兼容协议',models:[]},
];
export const vehicleModels=['星越L','星瑞','博越','星舰7','星耀7','星耀8','银河A7','银河E5','银河L6','银河L7','领克07','领克08','M7','TT','星愿'];
export function readDeviceMemory(raw:string|null):DeviceMemory|null{
 try{const value=JSON.parse(raw||'null');if(!value||typeof value.key!=='string'||!value.key.startsWith('device-')||typeof value.profile!=='string')return null;
  if(value.vehicle&&(!value.vehicle.name||typeof value.vehicle.name!=='string'||value.vehicle.name.length>100||/[\x00-\x1f]/.test(value.vehicle.name)))return null;return value;
 }catch{return null}
}
export function vehicleName(device:DeviceMemory|null){return device?.vehicle?.name||'我的车辆';}
export function validProfile(value:unknown):ConnectionProfile{return connectionProfiles.some(p=>p.value===value)?value as ConnectionProfile:'AUTO';}
