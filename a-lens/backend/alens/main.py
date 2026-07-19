"""A-Lens 서버 — 뷰모델 API + 프론트 정적 서빙.

실행: uvicorn alens.main:create_app --factory --port 8600 --reload
"""

from pathlib import Path

from fastapi import FastAPI
from fastapi.staticfiles import StaticFiles

from . import pipeline

_A_LENS = Path(__file__).resolve().parents[2]
_FRONT_DIST = _A_LENS / "frontend" / "dist"
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

    if _ASSETS.is_dir():
        app.mount("/assets", StaticFiles(directory=_ASSETS), name="assets")

    # 빌드된 프론트가 있으면 루트에서 서빙 (개발 중엔 Vite dev server가 /api를 프록시)
    if _FRONT_DIST.is_dir():
        app.mount("/", StaticFiles(directory=_FRONT_DIST, html=True), name="frontend")

    return app
