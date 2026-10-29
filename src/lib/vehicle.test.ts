import {describe,it,expect} from 'vitest';
import {readDeviceMemory,vehicleName,validProfile,connectionProfiles} from './vehicle';
import {validateAddress} from './recorder';
describe('remembered vehicle and connection schemes',()=>{
 it('keeps an identified model without treating it as connected',()=>{const memory=readDeviceMemory(JSON.stringify({key:'device-test',address:'192.168.43.8',profile:'A7',vehicle:{name:'星越L2026款',model:'星越L',year:2026,source:'device'}}));expect(vehicleName(memory)).toBe('星越L2026款');expect(memory).not.toHaveProperty('connected');});
 it('does not infer a car from a protocol or invalid cached session',()=>{expect(vehicleName(readDeviceMemory('{bad'))).toBe('我的车辆');expect(readDeviceMemory(JSON.stringify({vehicle:{name:'星越L2026款'}}))).toBeNull();});
 it('offers all six original schemes and automatic selection',()=>{expect(connectionProfiles.map(p=>p.value)).toEqual(['AUTO','A7','E5','L6','GENERIC1','GENERIC2','GENERIC3']);expect(validProfile('invalid')).toBe('AUTO');});
 it('lists complete original vehicle groups without vague placeholders',()=>{const a7=connectionProfiles.find(p=>p.value==='A7')!;expect(a7.models).toEqual(['银河A7','星耀7','M7','TT','2026款星舰7','2026款星瑞','2026款博越','2026款星越L','星愿']);expect(connectionProfiles.find(p=>p.value==='L6')!.models).toContain('2023/2024款领克08');expect(connectionProfiles.every(p=>!p.label.includes('等'))).toBe(true);});
 it('accepts only the original special factory addresses outside private ranges',()=>{expect(validateAddress('193.168.0.1')).toBe('193.168.0.1');expect(validateAddress('192.72.1.1')).toBe('192.72.1.1');expect(()=>validateAddress('193.168.0.2')).toThrow();});
});
