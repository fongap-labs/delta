import { defineConfig } from 'tsdown'

/** Emit the runtime entry declared by the shared first-party plugin package. */
export default defineConfig({
  entry: { index: 'lib/types/index.js' },
  outDir: 'lib',
  format: ['esm'],
  platform: 'node',
  target: 'es2024',
  fixedExtension: false,
  dts: false,
  clean: false,
})
