import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import { resolve, dirname } from 'path'
import { copyFileSync, mkdirSync, existsSync } from 'fs'
import { fileURLToPath } from 'url'
import { build as esbuild } from 'esbuild'

const rootDir = dirname(fileURLToPath(import.meta.url))

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
              entryPoints: [resolve(rootDir, 'src/content/index.ts')],
              bundle: true,
              format: 'iife',
              platform: 'browser',
              target: 'es2020',
              outfile: resolve(rootDir, 'dist/content.js'),
            }),
            esbuild({
              entryPoints: [resolve(rootDir, 'src/injected/index.ts')],
              bundle: true,
              format: 'iife',
              platform: 'browser',
              target: 'es2020',
              outfile: resolve(rootDir, 'dist/injected.js'),
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
          popup: resolve(rootDir, 'src/popup/index.html'),
          background: resolve(rootDir, 'src/background/index.ts')
        },
        output: {
          entryFileNames: '[name].js'
        }
      }
    }
  }
})
