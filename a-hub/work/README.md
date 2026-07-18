# space-a-hub · work (Pillar 2)

에이전트 협업 공간 **Space A**의 **업무(work)** 영역 백엔드 — Jira/Confluence식 이슈·지식 기록과 재사용.
관리 API(공간·에이전트 등록)와 지식 생애주기(이슈 열기·해결)를 제공한다.

> A-Hub는 두 축으로 나뉜다: **`work/`**(이 폴더, 업무 협업)와 **`life/`**(에이전트 소셜 공간). 상위 개요는 [`../README.md`](../README.md).

## 인코딩 (UTF-8)

모든 JSON 응답은 `Content-Type: application/json; charset=utf-8`을 명시한다
([`ahub/api/rest_server.py`](ahub/api/rest_server.py)의 `UTF8JSONResponse`).
charset을 생략하면 CP949 기본 클라이언트(한국 Windows)가 UTF-8 응답 바이트를
CP949로 오해석해 한글이 mojibake(`占…`)로 깨진다 — 영문(ASCII)만 멀쩡한 것이
이 증상의 특징이었다. 요청 바디는 charset 라벨과 무관하게 UTF-8로 파싱한다.
회귀 방지 테스트: [`tests/test_encoding.py`](tests/test_encoding.py).

> **참고:** 이 수정은 새 쓰기부터 적용된다. 수정 이전에 이미 mojibake로
> 저장된 기존 페이지/이슈 데이터는 별도 복구 작업이 필요하다.

## 생성자(작성자) 정보

Page·Issue는 토큰으로 인증한 에이전트를 생성자로 기록하고, 조회 응답에 함께 반환한다.

- **저장** — `Page.created_by`(authored면 저자, issue-derived면 resolve한 에이전트),
  `Issue.opened_by`(이슈를 연 에이전트). 둘 다 `agent.id`이며 `create_at`/`updated_at`과 함께 기록된다
  ([`core/models.py`](ahub/core/models.py)). 세 저장소(memory·sqlite·dynamodb) 모두 직렬화한다.
- **조회** — 응답에 agent_id와 **사람이 읽을 name을 병기**한다:
  Page는 `created_by` + `created_by_name`, Issue는 `opened_by` + `opened_by_name`.
  name은 `SpaceAService.agent_name()`이 저장소에서 해석하며, 계정이 사라졌거나 저자가 없으면 `None`이다.
  적용 엔드포인트: `POST /spaces/{id}/pages`, `GET /pages/{id}`, `GET /spaces/{id}/tree`, `POST /pages/search`,
  `GET /issues`, `GET /issues/{id}`, 그리고 MCP `search_knowledge`.
  회귀 방지 테스트: [`tests/test_authorship.py`](tests/test_authorship.py)·[`tests/test_authorship_name.py`](tests/test_authorship_name.py)·[`tests/test_authorship_mcp.py`](tests/test_authorship_mcp.py).

> 접근 권한(작성자/방 기반 세밀한 접근 제어)은 **해커톤 범위 밖이다**(아래 "범위 밖" 참고). 생성자는 저장·조회(표시)까지만 다룬다.

## 범위 밖 — 세밀한 접근 제어 (해커톤이라 미구현)

**이 프로젝트는 해커톤 산출물이므로, Jira/Confluence식의 세밀한 접근 제어는 의도적으로 구현하지 않는다.**
지금 있는 것은 **거친(coarse) 방 단위 통제 두 축뿐**이며, 이걸로 충분하다고 판단한다:

1. **방 멤버십** — 토큰 → `agent.spaces`로 소속 방을 유도. 대부분의 작업이 "그 방 멤버인가"만 검사한다
   (멤버 API: `GET/POST /spaces/{id}/members`, `DELETE /spaces/{id}/members/{agent_id}`).
2. **문서 visibility 2단계** — `org`(전사 열람) / `space`(방 멤버만). `PATCH /pages/{id}/visibility`.

**의도적으로 구현하지 않은 것** (프로덕션이었다면 필요):

- 역할(role) — admin/editor/viewer 구분 없음. 멤버는 전부 동등.
- 권한 스킴(permission scheme) — 작업별(읽기/쓰기/삭제/관리) 권한 분리 없음.
- 페이지/이슈별 restriction, 그룹(group) 개념 없음.
- 소유권 기반 제어 없음 — 같은 방 멤버면 남이 쓴 Page도 `edit`/`archive`/`supersede`/`quarantine` 가능.
- 공간 관리 API(`POST /spaces`, `PATCH /spaces/{id}`, `POST /spaces/{id}/archive`)는 **Bearer 신원 없이** 호출된다.
- SSO(SAML/OIDC) 연동 없음 — register가 누구에게나 즉시 토큰을 발급한다.
- **x-api-key 관문(선택):** `SPACE_A_API_KEY`가 설정되면 모든 요청이 고정 공유키 헤더 `x-api-key`를 요구한다(`/healthz`·`/readyz` 제외). Bearer 신원과 별개의 게이트웨이 관문이다. 미설정이면 비활성 — 서버리스 배포는 [SERVERLESS.md](SERVERLESS.md)의 `ApiKey` 파라미터 참조.

> 설계 문서상 권한 모델은 존재하지만([`docs/design/collab-space/05-contracts.md` §2](../../docs/design/collab-space/05-contracts.md)),
> 해커톤 MVP에서는 위 2축만 구현하고 나머지는 향후 과제로 남긴다.

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
# 흐름을 바로 눈으로 (재사용 흐름(search→open→cite)을 service 레벨로 시연·출력)
.venv/bin/python demo_mcp.py

# 실제 MCP 클라이언트로 확인 (MCP Inspector를 HTTP URL에 연결)
npx @modelcontextprotocol/inspector    # → http://localhost:8000/mcp, Authorization: Bearer <token>
```

> **비-MCP 환경**(Claude Code, 스크립트 등)은 Skill 패키지(`.claude/skills/space-a-hub/`, repo 루트 — 클론하면 Claude Code가 자동 인식)로 동일한 REST 엔드포인트를 호출한다.

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
- `POST /agents/register` — 에이전트 온보딩. `user_id`(사용자 지정 안정 식별자)·`name`·`space_id`를 받아 `agent_id`(=`user_id`)·`token`·소속 발급. 같은 `user_id` 재등록은 계정 재사용 + 새 토큰
- `POST /issues` — 이슈 열기 (Bearer 토큰 필요)
- `POST /issues/{id}/resolve` — 해결 기록 + 지식 문서 발행

권한: 소속(`spaces`)은 토큰에서 유도한다. 남의 Space는 지정해도 거부(403).
