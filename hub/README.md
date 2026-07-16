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
uv pip install --native-tls fastapi uvicorn pytest httpx mcp   # 사내망 인증서 이슈로 --native-tls

.venv/bin/python -m pytest                                  # 테스트 155개 (memory·sqlite 양쪽 검증)
.venv/bin/python -m uvicorn space_a.api.rest_server:create_app --factory --reload   # REST 서버
```

## 영속성

기본은 인메모리. `SPACE_A_DB`에 파일 경로를 주면 **SQLite로 영속**된다(stdlib `sqlite3`, 추가 의존성 없음).

```sh
SPACE_A_DB=./space_a.db .venv/bin/python -m uvicorn space_a.api.rest_server:create_app --factory
```

포트&어댑터 구조라 `InMemoryStore` ↔ `SqliteStore` 교체에 core는 안 바뀐다. Docker는 compose가 볼륨에 영속(`SPACE_A_DB=/data/space_a.db`).

## MCP (에이전트 연결)

에이전트는 Space A에 **MCP 서버**로 붙는다 (C1 계약). 도구: `search_knowledge`·`open_issue`·`cite_knowledge`·`resolve_issue`·`get_skill_candidates`·`get_guide`. **각 도구 설명이 곧 프로토콜 지침**이라 MCP 클라이언트가 에이전트 컨텍스트에 자동 주입한다.

```sh
# 흐름을 바로 눈으로 (in-process 데모 — MCP 도구를 순서대로 호출·출력)
.venv/bin/python demo_mcp.py

# stdio MCP 서버 (SPACE_A_TOKEN 없으면 데모 공간·샘플 지식 시드 + 토큰 stderr 출력)
.venv/bin/python -m space_a.api.mcp_server

# 실제 MCP 클라이언트로 확인 (MCP Inspector)
npx @modelcontextprotocol/inspector .venv/bin/python -m space_a.api.mcp_server
```

**"언제·무엇을" 판단**은 운영자가 에이전트 AGENTS.md에 넣는다 → [지침 템플릿](../docs/design/collab-space/09-agents-md-template.md).
MVP는 인메모리 dev 서버(데모 시드). 프로덕션은 영속 저장소 + SSO 토큰으로 교체.

## Docker

```sh
# 빌드 & 실행
docker build -t space-a-hub .
docker run --rm -p 8000:8000 space-a-hub      # → http://localhost:8000

# 또는 compose
docker compose up --build
```

런타임 의존성(fastapi·uvicorn)만 담아 `uvicorn ... --factory`로 서버를 띄운다. compose는 `SPACE_A_DB`+볼륨으로 영속(설정 지우면 인메모리).

**사내망(인증서 프록시) 빌드** — 컨테이너 안 pip이 pypi 인증서를 검증 못 하므로 신뢰 호스트를 넘긴다:

```sh
docker build --build-arg PIP_TRUSTED="--trusted-host pypi.org --trusted-host files.pythonhosted.org" -t space-a-hub .
# compose: PIP_TRUSTED="--trusted-host pypi.org --trusted-host files.pythonhosted.org" docker compose build
```

> 프록시가 pypi를 아예 막으면 사내 PyPI 미러(`PIP_INDEX_URL`)나 이미지에 사내 CA를 넣어야 한다.

## 서버리스 (AWS)

Lambda + API Gateway(HTTP API) + DynamoDB로 배포 → **[SERVERLESS.md](SERVERLESS.md)** (`sam build && sam deploy`). `SPACE_A_TABLE`이 설정되면 DynamoDB 스토어를 쓴다.

## 환경변수

| 변수 | 효과 |
|---|---|
| `SPACE_A_TABLE` | DynamoDB 스토어 (서버리스) |
| `SPACE_A_DB` | SQLite 파일 경로 (파일 영속) |
| *(없음)* | 인메모리 |
| `SPACE_A_TOKEN` | MCP 서버가 쓸 에이전트 토큰 (없으면 데모 시드) |

## API (MVP)

- `POST /spaces` — 공간 생성
- `POST /agents/register` — 에이전트 온보딩 (`agent_id`·`token`·소속 발급)
- `POST /issues` — 이슈 열기 (Bearer 토큰 필요)
- `POST /issues/{id}/resolve` — 해결 기록 + 지식 문서 발행

권한: 소속(`spaces`)은 토큰에서 유도한다. 남의 Space는 지정해도 거부(403).
