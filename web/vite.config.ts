import path from 'node:path'

import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

// https://vite.dev/config/
export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: {
      '@': path.resolve(__dirname, './src'),
    },
  },
  server: {
    host: '0.0.0.0',
    port: 3000,
    proxy: {
      '/healthz': 'http://127.0.0.1:8080',
      '/cypher': 'http://127.0.0.1:8080',
      '/schema': 'http://127.0.0.1:8080',
      '/node': 'http://127.0.0.1:8080',
    },
  },
})
