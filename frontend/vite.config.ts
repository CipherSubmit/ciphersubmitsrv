import { fileURLToPath, URL } from 'node:url'

import tailwindcss from '@tailwindcss/vite'
import vue from '@vitejs/plugin-vue'
import { defineConfig } from 'vite'

export default defineConfig({
  plugins: [tailwindcss(), vue()],
  resolve: {
    alias: {
      '@': fileURLToPath(new URL('./src', import.meta.url)),
    },
  },
  server: {
    host: '0.0.0.0',
    port: 5173,
    proxy: {
      '/api': {
        target: 'https://127.0.0.1:8443',
        changeOrigin: true,
        secure: false,
      },
      '/healthz': {
        target: 'https://127.0.0.1:8443',
        changeOrigin: true,
        secure: false,
      },
    },
  },
})