import { defineConfig } from 'tsup'

export default defineConfig({
  entry: [
    'src/extension.ts',
  ],
  format: ['cjs'],
  shims: false,
  dts: false,
  bundle: true,
  platform: 'node',
  external: [
    'vscode',
  ],
  noExternal: ['vscode-languageclient', 'ultra-runner'],
})
