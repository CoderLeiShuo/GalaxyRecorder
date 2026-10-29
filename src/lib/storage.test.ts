import { describe, expect, it } from 'vitest';
import {storageUsage,storageSummary,deviceVersions} from './storage';
describe('A7 storage usage', () => {
  const fields = { Sd: '0', TotalSize: '1000', usedNormalSpace: '500', usedEventSpace: '100', usedPhotoSpace: '0' };
  it('uses the real category bytes and accepts zero photo usage', () => {
    expect(storageUsage(fields)).toEqual({ total: 1000, used: 600, free: 400, values: [500, 100, 0] });
  });
  it('does not invent values for missing, inconsistent or unreadable storage', () => {
    expect(storageUsage({ ...fields, Sd: '1' })).toBeNull();
    expect(storageUsage({ ...fields, usedPhotoSpace: '-1' })).toBeNull();
    expect(storageUsage({ ...fields, usedNormalSpace: '2000' })).toBeNull();
    expect(storageUsage({})).toBeNull();
  });
  it('understands explicit byte units without mixing total and categories', () => {
    expect(storageUsage({ ...fields, TotalSize: '1 GB', usedNormalSpace: '512 MB', usedEventSpace: '0', usedPhotoSpace: '0' })?.free).toBe(512 * 1024 ** 2);
  });
});

describe('fields returned by the 2026 星越 L',()=>{
 it('parses CardInfo G units and SD0 READY without requiring category counters',()=>{
  const fields={'Camera.Menu.CardInfo.LifeTimeTotal':'58.1G','Camera.Menu.CardInfo.RemainLifeTime':'19.4G','Camera.Menu.SD0':'READY'};
  expect(storageSummary(fields)).toEqual({total:Math.round(58.1*1024**3),free:Math.round(19.4*1024**3),state:'正常'});expect(storageUsage(fields)).toBeNull();
 });
 it('accepts both controller version names and ignores empty fields',()=>{
  expect(deviceVersions({'Camera.Menu.FWversion':'0001',MCUVer:'',MCUversion:'2026.10'},'unknown')).toEqual({firmware:'0001',controller:'2026.10'});
  expect(deviceVersions({'Camera.Menu.FWversion':'0001'},'unknown').controller).toBeUndefined();
 });
 it('rejects impossible free capacity and keeps valid zero bytes',()=>{
  expect(storageSummary({TotalSize:'1G',FreeSize:'2G'}).free).toBeNull();expect(storageSummary({TotalSize:'1G',FreeSize:'0B'}).free).toBe(0);
 });
});
