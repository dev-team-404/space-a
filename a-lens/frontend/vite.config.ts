import { defineConfig } from 'vite'

// /api는 a-lens backend(포트 8600)로 프록시 — 프론트는 뷰모델만 받는다 (ADR 0003)
// 8600을 docker 컨테이너가 쓰고 있을 땐 별도 포트로 dev 백엔드를 띄우고
// `A_LENS_API_PORT=8601 npm run dev`로 그쪽을 보게 한다.
const apiTarget = `http://localhost:${process.env.A_LENS_API_PORT ?? 8600}`

export default defineConfig({
  // 빌드 산출 JS/CSS를 dist/static/ 으로 — 백엔드의 /assets(스프라이트 킷) 마운트와 경로가
  // 겹치면 번들이 404가 나 화면이 백지가 된다. dev는 vite가 직접 서빙해 무관.
  build: {
    assetsDir: 'static',
  },
  server: {
    port: 5279,
    strictPort: true,
    proxy: {
      '/api': apiTarget,
      '/assets': apiTarget, // 스프라이트 킷·배경 이미지 (backend가 a-lens/assets 서빙)
    },
  },
})
