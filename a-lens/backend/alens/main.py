"""A-Lens 서버 — 뷰모델 API + 프론트 정적 서빙.

실행: uvicorn alens.main:create_app --factory --port 8600 --reload
"""

import hashlib
from pathlib import Path

import httpx
from fastapi import Body, FastAPI, Header, HTTPException, Response
from fastapi.staticfiles import StaticFiles

from . import collector, life_client, pipeline, room_chat, rooms, settings, store

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

    # 방 오브젝트 클릭(spot=window|water) → 그 자리에 어울리는 잡담 2~3줄.
    # 창문은 view(방 프리셋)가 창밖 풍경을 정한다. LLM 없으면 폴백 문구.
    @app.get("/api/room-chat")
    def room_chat_lines(spot: str = "", view: str = "", space_name: str = ""):
        return room_chat.chat(spot=spot, view=view, space_name=space_name)

    # 마스코트 이미지 프록시 — 프론트가 Life 토큰을 들고 있지 않게 서버가 대신 가져온다.
    # 없으면 404 → 프론트는 기존 절차 생성 로봇으로 폴백한다.
    @app.get("/api/life-mascot/{agent_id}")
    def life_mascot(agent_id: str, if_none_match: str | None = Header(default=None)):
        # 캐시는 max-age가 아니라 **ETag 조건부 요청**으로 다룬다: 사진을 바꾸면 다음 조회에서
        # 곧바로 새 이미지가 나가고(no-cache), 안 바뀌었으면 304로 1MB 전송을 건너뛴다.
        # ETag는 Life가 주는 sha256이 있으면 그것, 없으면 본문 해시.
        png = life_client.mascot_png(agent_id)
        if not png:
            raise HTTPException(status_code=404, detail="mascot not found")
        digest = life_client.mascot_digest(agent_id) or hashlib.sha256(png).hexdigest()
        etag = f'W/"{digest[:32]}"'
        headers = {"Cache-Control": "no-cache", "ETag": etag}
        if if_none_match == etag:
            return Response(status_code=304, headers=headers)
        return Response(content=png, media_type="image/png", headers=headers)

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
        before_style = settings.get().get("summary_style")
        cfg = settings.update(clean)
        # 요약 길이(summary_style)가 바뀌면 기존 번역 캐시를 비워 새 스타일로 전체 재번역
        if cfg.get("summary_style") != before_style:
            st = store.get_store()
            if st is not None:
                st.clear_translations()
        collector.clear_cache()  # 새 URL/토큰/원천/스타일을 다음 요청부터 즉시 반영
        life_client.clear_cache()  # Life 연결값·별칭 매핑도 즉시 반영
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

    # ── 방(life 꾸미기) 공유 저장 — 모든 뷰어가 같은 방을 본다 ──
    @app.get("/api/life")
    def list_life():
        return {"rooms": rooms.list_rooms()}

    @app.post("/api/life")
    def save_life(config: dict = Body(...)):
        rooms.save_room(config)
        return {"ok": True}

    @app.delete("/api/life/{space_id}")
    def delete_life(space_id: str):
        rooms.delete_room(space_id)
        return {"ok": True}

    if _ASSETS.is_dir():
        app.mount("/assets", StaticFiles(directory=_ASSETS), name="assets")

    # 빌드된 프론트가 있으면 루트에서 서빙 (개발 중엔 Vite dev server가 /api를 프록시)
    if _FRONT_DIST.is_dir():
        app.mount("/", StaticFiles(directory=_FRONT_DIST, html=True), name="frontend")

    return app
