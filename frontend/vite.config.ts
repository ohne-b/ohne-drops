import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import tailwindcss from '@tailwindcss/vite';

export default defineConfig({
  plugins: [react(), tailwindcss()],
  build: { outDir: '../web', emptyOutDir: true, sourcemap: false },
  server: {
    fs: { allow: ['..'] },
    proxy: {
      '/api': 'http://127.0.0.1:8080',
      '/socket.io': { target: 'http://127.0.0.1:8080', ws: true },
      '/healthz': 'http://127.0.0.1:8080',
    },
  },
});
