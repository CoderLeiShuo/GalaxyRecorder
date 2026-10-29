import type {Appearance,AppSettings} from '../types';
export const defaultDeviceTitle='我的记录仪';
export function appearanceFromSettings(settings:Partial<AppSettings>):Appearance {
 return {deviceTitle:settings.deviceTitle||defaultDeviceTitle,deviceImage:settings.deviceImage||null,vehicleAliases:settings.vehicleAliases||{}};
}
export function readAppearance():Appearance {
 try{return appearanceFromSettings(JSON.parse(localStorage.getItem('galaxy-appearance')||'{}'));}catch{return appearanceFromSettings({});}
}
// Copy and normalize the selected image while the picker grant is valid. Canvas retains alpha.
export async function prepareDeviceImage(file:File):Promise<string>{
 if(file.size>12*1024*1024)throw Error('图片请小于 12 MB');
 const header=new Uint8Array(await file.slice(0,12).arrayBuffer());
 const png=header.length>=8&&[137,80,78,71,13,10,26,10].every((byte,i)=>header[i]===byte);
 const webp=header.length>=12&&String.fromCharCode(...header.slice(0,4))==='RIFF'&&String.fromCharCode(...header.slice(8,12))==='WEBP';
 if(!png&&!webp)throw Error('请选择 PNG 或 WebP 图片，支持透明背景');
 const url=await new Promise<string>((resolve,reject)=>{const reader=new FileReader();reader.onload=()=>resolve(String(reader.result));reader.onerror=()=>reject(Error('图片无法读取'));reader.readAsDataURL(file);});
 try{
  const image=new Image();image.src=url;await image.decode();
  if(!image.naturalWidth||!image.naturalHeight||image.naturalWidth*image.naturalHeight>40_000_000)throw Error('图片尺寸过大，请选择较小的图片');
  const scale=Math.min(1,1200/Math.max(image.naturalWidth,image.naturalHeight));
  const canvas=document.createElement('canvas');canvas.width=Math.max(1,Math.round(image.naturalWidth*scale));canvas.height=Math.max(1,Math.round(image.naturalHeight*scale));
  canvas.getContext('2d')!.drawImage(image,0,0,canvas.width,canvas.height);
  const data=canvas.toDataURL('image/png');if(data.length>4*1024*1024)throw Error('图片内容过大，请选择较小的图片');return data;
 }catch(error){throw error instanceof Error?error:Error('图片无法读取，请重新选择');}
}
