import { defineConfig } from 'vite';

export default defineConfig({
  server: {
    proxy: {
      '/health': 'http://127.0.0.1:8120',
      '/rpc': 'http://127.0.0.1:8120',
      '/schemas': 'http://127.0.0.1:8120',
      '/previews': 'http://127.0.0.1:8120',
      '/ws': {
        target: 'ws://127.0.0.1:8120',
        ws: true,
      },
    },
  },
  define: {
    __STUDIO_TOKEN__: JSON.stringify(process.env.STUDIO_SESSION_TOKEN ?? ''),
  },
});
