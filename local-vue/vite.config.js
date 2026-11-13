import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
export default defineConfig({
  plugins:[vue()],
  base:'/vue-v0.2.8/',
  build:{outDir:'../www/vue-v0.2.8',emptyOutDir:true,assetsDir:'assets',rollupOptions:{output:{entryFileNames:'assets/main-[hash].js',chunkFileNames:'assets/chunk-[hash].js',assetFileNames:'assets/[name]-[hash][extname]'}}}
})
