import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { defineConfig } from 'vite';
import vue from '@vitejs/plugin-vue';
import AutoImport from 'unplugin-auto-import/vite';
import Components from 'unplugin-vue-components/vite';
import { ElementPlusResolver } from 'unplugin-vue-components/resolvers';

const apiProxyTarget = process.env.XCPC_API_PROXY_TARGET || 'http://127.0.0.1:18080';
const appVersion = readFileSync(fileURLToPath(new URL('../../VERSION', import.meta.url)), 'utf8').trim();

export default defineConfig({
  define: {
    __APP_VERSION__: JSON.stringify(appVersion),
  },
  plugins: [
    vue(),
    AutoImport({
      resolvers: [ElementPlusResolver({ importStyle: false })],
      dts: 'src/types/auto-imports.d.ts',
    }),
    Components({
      resolvers: [ElementPlusResolver({ importStyle: false })],
      dts: 'src/types/components.d.ts',
    }),
  ],
  server: {
    port: 5173,
    proxy: {
      '/api': {
        target: apiProxyTarget,
        changeOrigin: true,
      },
    },
  },
  test: {
    environment: 'jsdom',
    setupFiles: ['./src/test/setup.ts'],
    restoreMocks: true,
    server: {
      deps: {
        inline: ['element-plus'],
      },
    },
  },
});
