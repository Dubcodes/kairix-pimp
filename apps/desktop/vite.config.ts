import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import path from 'node:path';

export default defineConfig({
  root: path.resolve(import.meta.dirname),
  plugins: [react()],
  resolve: {
    alias: {
      '@kairix-pimp/core': path.resolve(
        import.meta.dirname,
        '../../packages/core/src/index.ts',
      ),
    },
  },
  build: {
    outDir: path.resolve(import.meta.dirname, '../../dist'),
    emptyOutDir: true,
  },
  server: { port: 1420, strictPort: true },
});
