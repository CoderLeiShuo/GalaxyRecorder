import { describe, expect, it } from 'vitest';
import { clampSeek, parseWifiQr, playerTime } from './media';
describe('custom media controls', () => {
  it('bounds seeking and formats long recordings', () => {
    expect(clampSeek(-2, 120)).toBe(0); expect(clampSeek(300, 120)).toBe(120); expect(clampSeek(NaN, 120)).toBe(0);
    expect(playerTime(3661.8)).toBe('1:01:01'); expect(playerTime(59.9)).toBe('00:59');
  });
  it('reads escaped hotspot names without losing QR delimiters', () => {
    expect(parseWifiQr('WIFI:T:WPA;S:Car\\;WiFi;P:a\\:b\\;c;;')).toEqual({ ssid: 'Car;WiFi', password: 'a:b;c' });
    expect(() => parseWifiQr('https://example.com')).toThrow(); expect(() => parseWifiQr('WIFI:S:car;T:WEP;P:x;;')).toThrow();
  });
});
