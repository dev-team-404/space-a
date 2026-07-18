import { defineConfig } from 'vite'

// /api는 a-lens backend(포트 8600)로 프록시 — 프론트는 뷰모델만 받는다 (ADR 0003)
export default defineConfig({
  server: {
    port: 5173,
    proxy: {
      '/api': 'http://localhost:8600',
      '/assets': 'http://localhost:8600', // 스프라이트 킷·배경 이미지 (backend가 a-lens/assets 서빙)
    },
  },
})
