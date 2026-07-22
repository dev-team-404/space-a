"""A-Lens 서버 — 뷰모델 API + 프론트 정적 서빙.

실행: uvicorn alens.main:create_app --factory --port 8600 --reload
"""

from pathlib import Path

import httpx
from fastapi import Body, FastAPI
from fastapi.staticfiles import StaticFiles

from . import collector, pipeline, settings

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

    # ── 설정 (설정 창) — a-hub 연결 · LLM API 런타임 구성 ──
    @app.get("/api/settings")
    def get_settings():
        return settings.public()  # 비밀값은 *_set 불리언으로만

    @app.post("/api/settings")
    def post_settings(payload: dict = Body(default={})):
        # 비밀값(토큰·키)은 빈 문자열이면 "변경 안 함"으로 건너뛴다 — GET이 값을 안 주므로.
        clean = {
            k: v
            for k, v in (payload or {}).items()
            if not (k in settings.SECRET_KEYS and (v is None or v == ""))
        }
        cfg = settings.update(clean)
        collector.clear_cache()  # 새 URL/토큰/원천을 다음 요청부터 즉시 반영
        return settings.public(cfg)

    @app.post("/api/settings/test")
    def test_settings(payload: dict = Body(default={})):
        # 저장 전 폼 값으로도 테스트할 수 있게, 넘어온 값으로 현재 설정을 덮어 검사한다.
        cfg = dict(settings.get())
        for k, v in (payload or {}).items():
            if k in cfg and v not in (None, ""):
                cfg[k] = v
        out: dict = {}
        try:
            h = {"x-api-key": cfg["work_api_key"]} if cfg["work_api_key"] else {}
            if cfg["work_token"]:
                h["Authorization"] = f"Bearer {cfg['work_token']}"
            r = httpx.get(cfg["work_url"].rstrip("/") + "/spaces", headers=h, timeout=6)
            out["hub"] = {"ok": r.status_code < 400, "status": r.status_code}
        except Exception as e:  # noqa: BLE001
            out["hub"] = {"ok": False, "error": str(e)[:160]}
        try:
            h = {"Authorization": f"Bearer {cfg['llm_key']}"} if cfg["llm_key"] else {}
            r = httpx.get(cfg["llm_url"].rstrip("/") + "/models", headers=h, timeout=6)
            out["llm"] = {"ok": r.status_code < 400, "status": r.status_code}
        except Exception as e:  # noqa: BLE001
            out["llm"] = {"ok": False, "error": str(e)[:160]}
        return out

    if _ASSETS.is_dir():
        app.mount("/assets", StaticFiles(directory=_ASSETS), name="assets")

    # 빌드된 프론트가 있으면 루트에서 서빙 (개발 중엔 Vite dev server가 /api를 프록시)
    if _FRONT_DIST.is_dir():
        app.mount("/", StaticFiles(directory=_FRONT_DIST, html=True), name="frontend")

    return app
