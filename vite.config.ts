import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import { fileURLToPath } from 'node:url';

export default defineConfig({
  plugins: [svelte()],
  root: 'src',
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  build: {
    outDir: '../dist',
    emptyOutDir: true,
    // 멀티페이지: 2단계에서 mascot.html 엔트리 추가 (ESM 설정파일 — __dirname 없음)
    rollupOptions: {
      input: {
        chat: fileURLToPath(new URL('./src/chat.html', import.meta.url)),
        mascot: fileURLToPath(new URL('./src/mascot.html', import.meta.url)),
      },
    },
  },
});
