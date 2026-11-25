import { describe, expect, it } from 'vitest';
import type { RescanResult } from './bridge';
import { nextDiscoveryDelay } from './discovery-schedule';

const result = (
  stabilizing: number,
  needsFullVerification = 0,
  errors = 0,
): RescanResult => ({
  scanned: stabilizing + needsFullVerification,
  registered: 0,
  changed: 0,
  pending: stabilizing + needsFullVerification,
  pendingDetails: {
    stabilizing,
    needsFullVerification,
    changedDuringScan: 0,
  },
  errors,
  full: false,
  completedAt: '2026-11-18T00:00:00Z',
});

describe('background discovery scheduling', () => {
  it('rechecks actively stabilizing files after the stability window', () => {
    expect(nextDiscoveryDelay(result(28))).toBe(16_000);
  });

  it('backs off when files only require an explicit full verification', () => {
    expect(nextDiscoveryDelay(result(0, 28))).toBe(120_000);
    expect(nextDiscoveryDelay(result(0))).toBe(120_000);
  });

  it('retries failures without entering a tight loop', () => {
    expect(nextDiscoveryDelay(undefined)).toBe(30_000);
    expect(nextDiscoveryDelay(result(0, 0, 1))).toBe(30_000);
  });
});
