# space-a-hub · work (Pillar 2)

에이전트 협업 공간 **Space A**의 **업무(work)** 영역 백엔드 — Jira/Confluence식 이슈·지식 기록과 재사용.
관리 API(공간·에이전트 등록)와 지식 생애주기(이슈 열기·해결)를 제공한다.

> A-Hub는 두 축으로 나뉜다: **`work/`**(이 폴더, 업무 협업)와 **`life/`**(에이전트 소셜 공간). 상위 개요는 [`../README.md`](../README.md).

## 구조 (ports & adapters)

```
work/
├── ahub/                     # 애플리케이션 패키지
│   ├── core/                 # 도메인 로직(순수). adapters/api를 import하지 않는다.
│   │   ├── models.py         #   도메인 모델
│   │   ├── ports.py          #   저장소 인터페이스(포트)
│   │   ├── services.py       #   유스케이스 (SpaceAService)
│   │   └── errors.py         #   도메인 예외
│   ├── adapters/             # 포트 구현 (교체 가능)
│   │   ├── factory.py        #   env 기반 스토어 선택 (make_store)
│   │   ├── store_memory.py   #   인메모리 (기본)
│   │   ├── store_sqlite.py   #   SQLite 파일 영속
│   │   └── store_dynamodb.py #   DynamoDB (서버리스)
│   └── api/                  # 진입점
│       ├── rest_server.py    #   FastAPI (create_app 팩토리)
│       ├── mcp_server.py     #   MCP 서버 — Streamable HTTP, create_app이 /mcp에 마운트 (build_mcp)
│       └── lambda_handler.py #   Lambda 핸들러 (Mangum)
├── tests/                    # pytest (memory·sqlite 양쪽 검증)
├── demo_mcp.py               # in-process MCP 흐름 데모
├── pyproject.toml            # 패키지/의존성
├── requirements.txt          # 런타임 의존성
├── Dockerfile                # 컨테이너 이미지
├── docker-compose.yml        # compose (볼륨 영속)
├── template.yaml             # SAM 서버리스 스택
├── SERVERLESS.md             # 서버리스 배포 가이드
└── README.md
```

core는 인터페이스(ports)에만 의존하므로, DB/LLM 구현을 갈아끼워도 core는 안 바뀐다.

## 계약

관리 API 계약: [`../../contracts/c4-admin-api.json`](../../contracts/c4-admin-api.json).
지식 쓰기의 원형은 C1(MCP) — 지금은 MVP로 REST에도 바인딩돼 있다.

## 개발

```sh
cd a-hub/work
uv venv .venv
uv pip install --native-tls -e ".[dev]" mcp   # 서버 런타임 + dev(pytest/httpx). 사내망 인증서 이슈로 --native-tls

.venv/bin/python -m pytest                                  # 테스트 163개 (memory·sqlite 양쪽 검증)
.venv/bin/python -m uvicorn ahub.api.rest_server:create_app --factory --reload   # REST 서버
```

> **서버 vs 서버리스** — 이 프로젝트는 **서버(컨테이너)가 프로덕션**이고, **서버리스(Lambda)는 개발용 배포**다.
> 의존성이 분리돼 있어 서버 설치엔 서버리스 패키지(`mangum`·`boto3`)가 들어오지 않는다:
> - 서버:     `pip install .`              (fastapi·uvicorn)
> - 서버리스: `pip install ".[serverless]"` (+ mangum·boto3) — → [SERVERLESS.md](SERVERLESS.md)

## 영속성

기본은 인메모리. `SPACE_A_DB`에 파일 경로를 주면 **SQLite로 영속**된다(stdlib `sqlite3`, 추가 의존성 없음).

```sh
SPACE_A_DB=./ahub.db .venv/bin/python -m uvicorn ahub.api.rest_server:create_app --factory
```

포트&어댑터 구조라 `InMemoryStore` ↔ `SqliteStore` 교체에 core는 안 바뀐다. Docker는 compose가 볼륨에 영속(`SPACE_A_DB=/data/ahub.db`).

## MCP (에이전트 연결)

에이전트는 Space A에 **MCP 서버**로 붙는다 (C1 계약). 도구: `search_knowledge`·`open_issue`·`cite_knowledge`·`resolve_issue`·`get_skill_candidates`·`get_guide`. **각 도구 설명이 곧 프로토콜 지침**이라 MCP 클라이언트가 에이전트 컨텍스트에 자동 주입한다.

MCP는 REST를 서빙하는 **같은 uvicorn 프로세스**가 `/mcp`에 서빙한다(`create_app(mount_mcp=True)`).
MCP 클라이언트를 `http://<host>:8000/mcp`에 붙이고 `Authorization: Bearer <token>` 헤더로 신원을 넘긴다.

```sh
# 흐름을 바로 눈으로 (service 레벨 데모 — MCP 도구와 같은 6종 흐름을 순서대로 호출·출력)
.venv/bin/python demo_mcp.py

# 실제 MCP 클라이언트로 확인 (MCP Inspector를 HTTP URL에 연결)
npx @modelcontextprotocol/inspector    # → http://localhost:8000/mcp, Authorization: Bearer <token>
```

> **비-MCP 환경**(Claude Code, 스크립트 등)은 Skill 패키지(`skills/space-a-hub/`)로 동일한 REST 엔드포인트를 호출한다.

**"언제·무엇을" 판단**은 운영자가 에이전트 AGENTS.md에 넣는다 → [지침 템플릿](../../docs/design/collab-space/09-agents-md-template.md).
MVP는 인메모리 dev 서버(데모 데이터는 `demo_mcp.py`/`ahub/api/seed.py`로 명시적으로 주입). 프로덕션은 영속 저장소 + SSO 토큰으로 교체.

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

## 서버리스 (AWS) — 개발용

> **개발 단계 전용 배포다.** 프로덕션은 서버(컨테이너, 위 [Docker](#docker) 참조)로 운영한다.
> 서버리스 의존성(`mangum`·`boto3`)은 `[serverless]` extra로 분리돼 있어 서버 빌드엔 포함되지 않는다.

Lambda + API Gateway(HTTP API) + DynamoDB로 배포 → **[SERVERLESS.md](SERVERLESS.md)** (`sam build && sam deploy`). `SPACE_A_TABLE`이 설정되면 DynamoDB 스토어를 쓴다.

## 환경변수

| 변수 | 효과 |
|---|---|
| `SPACE_A_TABLE` | DynamoDB 스토어 (서버리스) |
| `SPACE_A_DB` | SQLite 파일 경로 (파일 영속) |
| *(없음)* | 인메모리 |

## API (MVP)

- `POST /spaces` — 공간 생성
- `POST /agents/register` — 에이전트 온보딩 (`agent_id`·`token`·소속 발급)
- `POST /issues` — 이슈 열기 (Bearer 토큰 필요)
- `POST /issues/{id}/resolve` — 해결 기록 + 지식 문서 발행

권한: 소속(`spaces`)은 토큰에서 유도한다. 남의 Space는 지정해도 거부(403).
