import { describe, expect, it } from 'vitest';
import {
  compareVersions,
  displayVersion,
  parseVersion,
  validateClassification,
} from './release-policy.mjs';

describe('release policy', () => {
  it('orders numeric components rather than decimals or strings', () => {
    expect(compareVersions('1.22.0', '1.3.0')).toBeGreaterThan(0);
    expect(compareVersions('2.0.0', '1.99.0')).toBeGreaterThan(0);
    expect(compareVersions('0.2.0', '0.1.1-6')).toBeGreaterThan(0);
  });

  it('presents zero-patch versions without technical suffixes', () => {
    expect(displayVersion('0.2.0')).toBe('0.2');
    expect(displayVersion('1.22.0')).toBe('1.22');
    expect(displayVersion('0.1.1-6')).toBe('0.1.1-6');
  });

  it('requires an explicit classification and reserves whole majors for stable', () => {
    expect(validateClassification('0.2.0', 'development')).toBe(true);
    expect(validateClassification('1.1.0', 'development')).toBe(true);
    expect(validateClassification('1.0.0', 'stable')).toBe(true);
    expect(() => validateClassification('1.0.0', 'development')).toThrow();
    expect(() => validateClassification('1.2.0', 'stable')).toThrow();
    expect(() => validateClassification('0.2.0', 'stable')).toThrow();
  });

  it('retains parsing compatibility with legacy numeric prereleases', () => {
    expect(parseVersion('0.1.1-5').prerelease).toBe('5');
  });
});
