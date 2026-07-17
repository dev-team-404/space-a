"""A-Lens 서버 — 뷰모델 API + 프론트 정적 서빙.

실행: uvicorn alens.main:create_app --factory --port 8600 --reload
"""

from pathlib import Path

from fastapi import FastAPI, HTTPException
from fastapi.staticfiles import StaticFiles

from . import pipeline

_A_LENS = Path(__file__).resolve().parents[2]
_FRONT_DIST = _A_LENS / "frontend" / "dist"
_PROTOTYPE = _A_LENS / "prototype"
_ASSETS = _A_LENS / "assets"


def create_app() -> FastAPI:
    app = FastAPI(title="A-Lens", version="0.1.0")

    @app.get("/api/health")
    def health():
        return {"ok": True}

    @app.get("/api/lobby")
    def lobby():
        return pipeline.lobby_view()

    @app.get("/api/spaces/{space_id}")
    def space(space_id: str, tier: str = "member"):
        return pipeline.space_view(space_id, tier)

    # ── C2 wire 브리지 — 프로토타입 UI(/prototype)에 실데이터 공급 ──
    # hub 원천일 때만 200. 픽스처 모드면 404 → 프로토타입은 내장 가짜 데이터 유지.

    def _c2(payload):
        if payload is None:
            raise HTTPException(status_code=404, detail="not in hub mode — prototype falls back to its fake data")
        return payload

    @app.get("/api/c2/spaces")
    def c2_spaces():
        return _c2(pipeline.c2_spaces())

    @app.get("/api/c2/space-details")
    def c2_space_details():
        return _c2(pipeline.c2_space_details())

    @app.get("/api/c2/activity")
    def c2_activity():
        return _c2(pipeline.c2_activity())

    @app.get("/api/c2/reuse-events")
    def c2_reuse_events():
        return _c2(pipeline.c2_reuse_events())

    @app.get("/api/c2/stats")
    def c2_stats():
        return _c2(pipeline.c2_stats())

    # 프로토타입 UI — 실데이터 확인용 (http 서빙일 때 c2-live.js가 위 브리지로 덮어씀)
    if _PROTOTYPE.is_dir():
        app.mount("/prototype", StaticFiles(directory=_PROTOTYPE, html=True), name="prototype")
    if _ASSETS.is_dir():
        app.mount("/assets", StaticFiles(directory=_ASSETS), name="assets")

    # 빌드된 프론트가 있으면 루트에서 서빙 (개발 중엔 Vite dev server가 /api를 프록시)
    if _FRONT_DIST.is_dir():
        app.mount("/", StaticFiles(directory=_FRONT_DIST, html=True), name="frontend")

    return app
