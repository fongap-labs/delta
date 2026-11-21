import { defineConfig } from 'tsdown'

/** Preserve the Cordis Host entry and optional plugin settings client. */
export default defineConfig({
  entry: {
    index: 'lib/types/index.js',
    client: 'lib/types/client.js',
  },
  outDir: 'lib',
  format: ['esm'],
  platform: 'node',
  target: 'es2024',
  fixedExtension: false,
  dts: false,
  clean: false,
})
