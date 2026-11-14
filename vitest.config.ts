import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    include: [
      'packages/**/*.test.ts',
      'apps/**/*.test.ts',
      'apps/**/*.test.tsx',
      'tools/**/*.test.js',
    ],
    environment: 'node',
    setupFiles: ['apps/desktop/src/test/setup.ts'],
  },
});
