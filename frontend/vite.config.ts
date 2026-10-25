import { fileURLToPath, URL } from 'node:url'
import tailwindcss from '@tailwindcss/vite'
import react from '@vitejs/plugin-react'
import { defineConfig } from 'vite'

// Set by `cargo tauri android dev` so the phone can reach this dev server.
const host = process.env.TAURI_DEV_HOST

// https://vite.dev/config/
export default defineConfig({
  plugins: [react(), tailwindcss()],
  resolve: {
    alias: {
      '@': fileURLToPath(new URL('./src', import.meta.url)),
      // A bare name, so `src/foliate-js.d.ts` can type this plain-JS submodule.
      'foliate-js': fileURLToPath(new URL('./vendor/foliate-js', import.meta.url)),
    },
  },
  // Tauri expects a fixed port and its own error output.
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: 'ws', host, port: 1421 } : undefined,
    // `pnpm dev` in a browser talks to a locally running shelf-server.
    proxy: { '/api': 'http://localhost:8080' },
  },
})
