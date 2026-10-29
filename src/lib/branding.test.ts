import {describe,it,expect} from 'vitest';
import {displayAppName} from './branding';
describe('application name migration',()=>{
 it('updates the old default while keeping a name chosen by the user',()=>{
  expect(displayAppName(null)).toBe('吉行影记');
  expect(displayAppName('银河记录仪')).toBe('吉行影记');
  expect(displayAppName('小雷的星越 L')).toBe('小雷的星越 L');
 });
});
