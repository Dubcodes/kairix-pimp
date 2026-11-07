import { describe, expect, it } from 'vitest';
import {
  compareVersions,
  displayVersion,
  parseVersion,
  validateClassification,
} from './release-policy.mjs';
import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import process from 'node:process';
import { fileURLToPath, URL } from 'node:url';

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

  it('exposes fail-closed channel comparison results to release workflows', () => {
    const policy = fileURLToPath(
      new URL('./release-policy.mjs', import.meta.url),
    );
    const compare = (candidate, current) =>
      execFileSync(process.execPath, [policy, 'compare', candidate, current], {
        encoding: 'utf8',
      }).trim();
    expect(compare('1.22.0', '1.3.0')).toBe('newer');
    expect(compare('0.2.0', '0.2.0')).toBe('equal');
    expect(compare('0.1.1-6', '0.2.0')).toBe('older');
  });

  it('serializes publication and gives Stable payloads both channel targets', () => {
    const publicRelease = readFileSync(
      new URL('../.github/workflows/public-beta-release.yml', import.meta.url),
      'utf8',
    );
    const signedUpdate = readFileSync(
      new URL('../.github/workflows/signed-beta-update.yml', import.meta.url),
      'utf8',
    );
    expect(publicRelease).toContain(
      'group: public-windows-release-publication',
    );
    expect(signedUpdate).toContain('group: signed-update-publication');
    expect(signedUpdate).toContain("$platforms['development-channel']");
    expect(signedUpdate).toContain('release-policy.mjs compare');
  });
});
