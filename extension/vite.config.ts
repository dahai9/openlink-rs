import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import { resolve } from 'path'
import { copyFileSync, mkdirSync, existsSync } from 'fs'
import { build as esbuild } from 'esbuild'

export default defineConfig(({ mode }) => {
  const manifestFile = mode === 'firefox' ? 'public/manifest.firefox.json' : 'public/manifest.json'

  return {
    plugins: [
      react(),
      {
        name: 'copy-files',
        async closeBundle() {
          await Promise.all([
            esbuild({
              entryPoints: [resolve(__dirname, 'src/content/index.ts')],
              bundle: true,
              format: 'iife',
              platform: 'browser',
              target: 'es2020',
              outfile: resolve(__dirname, 'dist/content.js'),
            }),
            esbuild({
              entryPoints: [resolve(__dirname, 'src/injected/index.ts')],
              bundle: true,
              format: 'iife',
              platform: 'browser',
              target: 'es2020',
              outfile: resolve(__dirname, 'dist/injected.js'),
            }),
          ])

          mkdirSync('dist', { recursive: true })
          copyFileSync(manifestFile, 'dist/manifest.json')
          if (existsSync('dist/src/popup/index.html')) {
            copyFileSync('dist/src/popup/index.html', 'dist/popup.html')
          }
        }
      }
    ],
    build: {
      outDir: 'dist',
      emptyOutDir: true,
      rollupOptions: {
        input: {
          popup: resolve(__dirname, 'src/popup/index.html'),
          background: resolve(__dirname, 'src/background/index.ts')
        },
        output: {
          entryFileNames: '[name].js'
        }
      }
    }
  }
})
