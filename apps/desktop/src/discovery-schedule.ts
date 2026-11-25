import type { RescanResult } from './bridge';

export const nextDiscoveryDelay = (
  result: RescanResult | undefined,
): number => {
  if (!result || result.errors > 0) return 30_000;
  if (result.pendingDetails.stabilizing > 0) return 16_000;
  return 120_000;
};
