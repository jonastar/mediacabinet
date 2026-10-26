import react from '@vitejs/plugin-react'
import { defineConfig } from 'vite'

// The Rust server listens on 8420 by default (CABINET_BIND).
export default defineConfig({
  plugins: [react()],
  // hls.js is its own chunk, loaded only when a stream needs it.
  build: { chunkSizeWarningLimit: 600 },
  server: {
    proxy: {
      '/api': 'http://127.0.0.1:8420',
    },
  },
})
