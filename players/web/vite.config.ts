import { fileURLToPath } from 'node:url'
import { transformAsync } from '@babel/core'
import legacy from '@vitejs/plugin-legacy'
import { defineConfig, type Plugin } from 'vite'
import * as tokens from './src/tokens.ts'

const UNITS: Record<string, string> = { color: '', duration: 'ms', layoutNumber: '' }

/**
 * Writes tokens into the CSS as fixed values, because older TVs have no CSS
 * variables: `token(color.accent)` becomes `#2bb5a6`, `token(space.space16)` `16px`.
 */
function fixedTokens(): Plugin {
  return {
    name: 'fixed-tokens',
    enforce: 'pre',
    transform(code, id) {
      if (!id.split('?')[0].endsWith('.css')) {
        return null
      }
      return code.replace(/token\((\w+)\.(\w+)\)/g, (_, group: string, name: string) => {
        // `layoutNumber` is a layout value that has no unit, such as a scale.
        const source = group === 'layoutNumber' ? 'layout' : group
        const value = (tokens as unknown as Record<string, Record<string, unknown>>)[source]?.[name]
        if (value === undefined) {
          throw new Error(`No token ${group}.${name}`)
        }
        return `${value}${UNITS[group] ?? 'px'}`
      })
    },
  }
}

/**
 * The legacy plugin builds its polyfills apart, skipping Babel, so the bundler's
 * own helpers stay as arrow functions, which Chromium 38 can't parse.
 */
function lowerPolyfills(): Plugin {
  return {
    name: 'lower-polyfills',
    enforce: 'post',
    async generateBundle(_options, bundle) {
      for (const chunk of Object.values(bundle)) {
        if (chunk.type === 'chunk' && chunk.fileName.includes('polyfills-legacy')) {
          const lowered = await transformAsync(chunk.code, {
            babelrc: false,
            configFile: false,
            compact: true,
            comments: false,
            plugins: ['@babel/plugin-transform-arrow-functions'],
          })
          chunk.code = lowered?.code ?? chunk.code
        }
      }
    },
  }
}

/** `crossorigin` scripts are blocked from a file, which is where a packaged app runs. */
function plainScripts(): Plugin {
  return {
    name: 'plain-scripts',
    enforce: 'post',
    transformIndexHtml: {
      order: 'post',
      handler: (html) => html.replace(/ crossorigin/g, ''),
    },
  }
}

export default defineConfig({
  // A packaged app loads from a file, so nothing can start at the root.
  base: './',
  plugins: [
    fixedTokens(),
    // The oldest engines planned: LG's 2016 webOS 3. See docs/clients.md.
    legacy({ targets: ['chrome >= 38'], renderModernChunks: false }),
    lowerPolyfills(),
    plainScripts(),
  ],
  resolve: {
    alias: { '@core': fileURLToPath(new URL('../../web/src/core', import.meta.url)) },
  },
  oxc: { jsx: { runtime: 'automatic', importSource: 'preact' } },
  build: {
    cssTarget: 'chrome38',
    // Terser, because the default minifier turns legacy chunks back into ES2015.
    minify: 'terser',
    chunkSizeWarningLimit: 600,
  },
  server: { port: 5174 },
})
