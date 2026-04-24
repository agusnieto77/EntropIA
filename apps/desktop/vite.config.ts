import { defineConfig } from 'vite'
import { svelte } from '@sveltejs/vite-plugin-svelte'
import { resolve } from 'path'

export default defineConfig({
  plugins: [svelte()],
  optimizeDeps: {
    // Restrict dep-scan to the real frontend entry.
    // Without this, Vite may crawl every HTML file under apps/desktop,
    // including Rustdoc output under src-tauri/target/doc, which on Windows
    // can trigger EMFILE loops during dependency re-optimization.
    entries: ['index.html'],
  },
  resolve: {
    alias: {
      $lib: resolve(__dirname, './src/lib'),
    },
  },
  // Tauri expects a fixed port in dev
  server: {
    port: 1420,
    strictPort: true,
    watch: {
      // Watch packages/ui for cross-package HMR
      ignored: ['**/src-tauri/**'],
    },
  },
  build: {
    target: 'chrome105',
    minify: !process.env.TAURI_DEBUG ? 'esbuild' : false,
    sourcemap: !!process.env.TAURI_DEBUG,
  },
})
