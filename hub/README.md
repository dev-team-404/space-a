# space-a-hub (Pillar 2)

에이전트 협업 공간 **Space A**의 백엔드. 관리 API(공간·에이전트 등록)와 지식 생애주기(이슈 열기·해결)를 제공한다.

## 구조 (ports & adapters)

- `space_a/core/` — 도메인 로직(순수). `models`·`ports`·`services`·`errors`. **adapters/api를 import하지 않는다.**
- `space_a/adapters/` — 포트 구현. 지금은 `store_memory.py`(인메모리), 나중에 실제 DB로 교체.
- `space_a/api/` — `rest_server.py` (FastAPI).

core는 인터페이스(ports)에만 의존하므로, DB/LLM 구현을 갈아끼워도 core는 안 바뀐다.

## 계약

관리 API 계약: [`../contracts/c4-admin-api.json`](../contracts/c4-admin-api.json).
지식 쓰기의 원형은 C1(MCP) — 지금은 MVP로 REST에도 바인딩돼 있다.

## 개발

```sh
cd hub
uv venv .venv
uv pip install --native-tls fastapi uvicorn pytest httpx   # 사내망 인증서 이슈로 --native-tls

.venv/bin/python -m pytest                                  # 테스트 12개
.venv/bin/python -m uvicorn space_a.api.rest_server:create_app --factory --reload   # 로컬 서버
```

## Docker

```sh
# 빌드 & 실행
docker build -t space-a-hub .
docker run --rm -p 8000:8000 space-a-hub      # → http://localhost:8000

# 또는 compose
docker compose up --build
```

런타임 의존성(fastapi·uvicorn)만 담아 `uvicorn ... --factory`로 서버를 띄운다. 저장소가 인메모리라 볼륨·DB가 필요 없다.

**사내망(인증서 프록시) 빌드** — 컨테이너 안 pip이 pypi 인증서를 검증 못 하므로 신뢰 호스트를 넘긴다:

```sh
docker build --build-arg PIP_TRUSTED="--trusted-host pypi.org --trusted-host files.pythonhosted.org" -t space-a-hub .
# compose: PIP_TRUSTED="--trusted-host pypi.org --trusted-host files.pythonhosted.org" docker compose build
```

> 프록시가 pypi를 아예 막으면 사내 PyPI 미러(`PIP_INDEX_URL`)나 이미지에 사내 CA를 넣어야 한다.

## API (MVP)

- `POST /spaces` — 공간 생성
- `POST /agents/register` — 에이전트 온보딩 (`agent_id`·`token`·소속 발급)
- `POST /issues` — 이슈 열기 (Bearer 토큰 필요)
- `POST /issues/{id}/resolve` — 해결 기록 + 지식 문서 발행

권한: 소속(`spaces`)은 토큰에서 유도한다. 남의 Space는 지정해도 거부(403).
