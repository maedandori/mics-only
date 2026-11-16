import { defineConfig } from 'vite'
import { resolve } from 'node:path'
export default defineConfig({
  root:'original',
  base:'/vue-v0.2.17/',
  build:{
    outDir:'../../www/vue-v0.2.17',
    emptyOutDir:true,
    assetsDir:'assets',
    rollupOptions:{
      input:{
        index:resolve('original/index.html'),
        'change-schedule':resolve('original/change-schedule.html'),
        'status-colors':resolve('original/status-colors.html'),
        'change-aliases':resolve('original/change-aliases.html')
      },
      output:{entryFileNames:'assets/[name]-[hash].js',chunkFileNames:'assets/chunk-[hash].js',assetFileNames:'assets/[name]-[hash][extname]'}
    }
  }
})
