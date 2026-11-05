import react from '@vitejs/plugin-react'
import { defineConfig } from 'vite'

// Built assets go into the server crate, which embeds them in the binary.
// In dev, /api is proxied to a running `agentcam serve --no-open`.
export default defineConfig({
  plugins: [react()],
  build: {
    outDir: '../crates/agentcam-server/ui-dist',
    emptyOutDir: true,
  },
  server: {
    proxy: {
      '/api': 'http://127.0.0.1:4747',
    },
  },
})
