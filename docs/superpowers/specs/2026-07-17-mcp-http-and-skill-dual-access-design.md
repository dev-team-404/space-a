# A-Hub(work) 이중 입구 설계 — MCP HTTP 전환 + Skill(REST) 접근

- **일자:** 2026-07-17
- **대상:** `a-hub/work` (space-a-hub 백엔드, Pillar 2)
- **상태:** 설계 승인됨 → 구현 계획 대기

## 0. 배경 · 문제

현재 `a-hub/work`의 MCP 구현은 **stdio 전송 + 단일 토큰(`SPACE_A_TOKEN`)** 방식이다.
이 방식은 클라이언트에 Python + 이 코드가 있어야 붙을 수 있어, "Python 없는 환경에서도
붙는다"는 MCP의 이점을 스스로 무효화한다.

무엇보다 이는 **자기 계약(C1)을 절반만 지킨 상태다.** 계약 문서
[`05-contracts.md`](../../design/a-hub/05-contracts.md) §3.1은 이미 이렇게 확정해 두었다:

| 전송 | 방식 | 위치 |
|---|---|---|
| **HTTP (원격, 기본)** | `Authorization: Bearer <token>` | 프로덕션 기본 |
| stdio (로컬 개발) | env `SPACE_A_TOKEN` | dev 전용 |

> "모든 C1 호출은 인증된다. **신원은 요청 파라미터가 아니라 전송 계층에서 온다.**"

즉 계약이 "기본"이라 정한 HTTP + per-request Bearer 경로가 미구현이다. 이 설계는 그 구현을
따라잡고(catch-up), 동시에 MCP를 못 쓰는/안 쓰는 환경을 위한 **Skill(REST 호출) 입구**를 추가한다.

## 1. 목표

A-Hub에 **두 개의 입구**를 제공한다 — 같은 서버, 같은 도메인 로직, 같은 per-request Bearer 인증.

- **입구 A (MCP):** MCP 클라이언트 환경(Claude Desktop 등) → `/mcp` (Streamable HTTP)
- **입구 B (Skill):** 비-MCP 환경(Claude Code, 스크립트 등) → Skill 패키지가 REST 호출

두 입구는 **경쟁이 아니라 같은 기능의 두 전송 경로**다. 도메인 로직·인증·데이터가 전부 서버에
있으므로 어느 입구로 들어와도 동일하게 동작하고 데이터도 공유된다.

### 비목표 (YAGNI)

- 사내 SSO(SAML/OIDC) 연동 — 계약대로 정적 토큰 → `spaces[]` 매핑 유지, 헤더/클레임 구조만 고정.
- Lambda에서 MCP 서빙 — 서버리스는 REST만 (아래 §5).
- 관리 API(공간 생성·에이전트 등록)의 Skill 노출 — Skill 범위는 MCP 도구 6종과 동일.

## 2. 아키텍처

```
┌─ uvicorn 1개 프로세스 (프로덕션, 컨테이너) ──────────────────┐
│  create_app() → FastAPI                                     │
│    ├── /            REST 라우트          ← Skill이 호출 (curl/http)│
│    └── /mcp         MCP (Streamable HTTP) ← MCP 클라이언트가 호출  │
│                                                             │
│         공유: SpaceAService + make_store (동일 스토어)        │
└──────────────────────────────────────────────────────────────┘

입구 A: MCP 환경    → /mcp
입구 B: 비-MCP 환경 → Skill 패키지 → REST

Lambda (서버리스, 개발용): REST만 노출. Skill 입구는 여기서도 동작. /mcp 마운트 안 함.
```

## 3. 컴포넌트별 변경

### 3.1 MCP HTTP 전환 — `ahub/api/mcp_server.py`

**제거:**
- stdio `main()` 및 `if __name__ == "__main__"` 블록
- `SPACE_A_TOKEN` 단일 토큰 분기
- 데모 시드 로직 전부 (빈 스토어 자동 시드) → 필요 시 `demo_mcp.py` 계열로만

**변경:** `build_mcp(service)`가 요청별 Bearer로 신원을 유도한다.

```python
def build_mcp(service=None) -> FastMCP:
    service = service or SpaceAService(make_store())
    mcp = FastMCP("space-a-hub")

    def _token(ctx: Context) -> str:
        # 전송 계층에서 신원 (계약 §3.1). REST의 _bearer와 동일 규칙.
        req = ctx.request_context.request           # Starlette Request
        return _bearer(req.headers.get("authorization"))

    @mcp.tool()
    def search_knowledge(query, space_id=None, limit=3, ctx: Context = None):
        res = service.search_knowledge(_token(ctx), query, space_id=space_id, limit=limit)
        ...
    # open_issue / cite_knowledge / resolve_issue / get_skill_candidates / get_guide 동일 패턴
    return mcp
```

**라이브러리 검증 (설계 핵심 가정, 확인 완료 — `mcp` 1.27.0):**
- `FastMCP.streamable_http_app()` 존재 → ASGI 앱으로 마운트 가능.
- Streamable HTTP transport가 Starlette `Request`를 생성(streamable_http_manager.py:227)하고,
  lowlevel 서버가 이를 `request_context.request`로 전달(server.py:765).
- 따라서 도구 함수가 `ctx: Context`를 받아 `ctx.request_context.request.headers["authorization"]`로
  요청별 Bearer에 닿는다.

**에러 매핑:** REST는 `SpaceAError`→HTTP status 매핑(`rest_server.py:102`)이 있다. MCP 도구에서
도메인 에러(`Unauthorized` 등)가 나면 이를 잡아 MCP 규격 에러로 변환하는 얇은 처리를 둔다.

**공유:** `_bearer()` 토큰 파싱·`Unauthorized` 규칙을 REST와 MCP가 공유한다. 현재 `_bearer`는
`rest_server.py`의 모듈 함수이므로, 순환 import를 피해 공용 헬퍼 모듈(예: `ahub/api/_auth.py`)로
추출하고 REST·MCP가 함께 import한다.

### 3.2 한 프로세스 조립 — `create_app`에 `/mcp` 마운트

```python
def create_app(service=None, *, mount_mcp=True) -> FastAPI:
    service = service or SpaceAService(make_store())
    app = FastAPI(title="Space A Hub")
    ...  # 기존 REST 라우트

    if mount_mcp:
        from ..api.mcp_server import build_mcp        # 지연 import
        mcp_app = build_mcp(service).streamable_http_app()
        app.mount("/mcp", mcp_app)
        # Streamable HTTP 세션 매니저를 lifespan에서 기동해야 함 → FastAPI lifespan과 결합
    return app
```

**결정:**
1. **`mount_mcp` 플래그** — 기본 `True`(서버/컨테이너). Lambda는 `create_app(mount_mcp=False)`.
2. **service 공유** — REST와 MCP가 같은 `service` 인스턴스 → 같은 스토어, 데이터 일관성.
3. **Lifespan 결합** — Streamable HTTP 세션 매니저를 lifespan에서 기동 (라이브러리 표준 패턴).
4. **지연 import** — `mcp` import를 `if mount_mcp:` 안에 두어 Lambda(`mount_mcp=False`) 번들에
   `mcp`가 안 들어가게 한다.

### 3.3 Lambda 진입점 — `ahub/api/lambda_handler.py`

```python
handler = Mangum(create_app(mount_mcp=False))   # REST만
```

### 3.4 Skill 패키지 (비-MCP 입구) — `a-hub/work/skills/space-a-hub/`

```
a-hub/work/skills/space-a-hub/
  SKILL.md              # 트리거 + 언제/어떻게 + 호출 예시
  references/
    endpoints.md        # REST 엔드포인트 ↔ 작업 매핑 상세
```

**SKILL.md 골자:**
- 트리거: 에이전트 협업 공간 검색/이슈/지식 재사용 관련 작업
- 워크플로 지침(설계 의도와 일치, `09-agents-md-template`): "막히면 `search_knowledge` 먼저
  → `open_issue` → `cite`/`resolve`"
- 호출: 서버 base URL + `Authorization: Bearer <token>`로 REST 호출. 6개 작업 각각 curl 예시
- 설정값: base URL·토큰은 환경변수(예: `SPACE_A_HUB_URL`, `SPACE_A_TOKEN`)로 읽도록 안내 (하드코딩 금지)

**범위 = MCP 도구 6종과 1:1 (REST는 이미 전부 구현되어 있음):**

| 작업 | MCP 도구 | REST 엔드포인트 |
|---|---|---|
| 가이드 읽기 | `get_guide` | `GET /spaces/{id}/guide` |
| 검색 | `search_knowledge` | `POST /pages/search` |
| 이슈 열기 | `open_issue` | `POST /issues` |
| 인용(재사용) | `cite_knowledge` | `POST /issues/{id}/cite` |
| 해결 | `resolve_issue` | `POST /issues/{id}/resolve` |
| 스킬 후보 | `get_skill_candidates` | `GET /skills/candidates` |

## 4. 데이터 흐름 · 인증

```
에이전트 A --Bearer tokA--> /mcp  --> _token(ctx) --> service --> spaces[A] 범위
에이전트 B --Bearer tokB--> POST /issues (Skill) --> _bearer --> service --> spaces[B] 범위
```

- 신원은 **전송 계층**(HTTP Authorization 헤더)에서만 온다 — 도구/요청 파라미터로 받지 않는다(계약 §3.1).
- MCP·REST가 **동일한** 토큰 파싱(`_bearer`)과 도메인 서비스·스토어를 공유한다.
- 토큰 → `{ user_id, agent_id, spaces[] }`, `spaces[]`가 권한의 전부.

## 5. 서버 vs 서버리스

| | 서버 (컨테이너, 프로덕션) | 서버리스 (Lambda, 개발용) |
|---|---|---|
| 진입점 | `create_app(mount_mcp=True)` (uvicorn) | `create_app(mount_mcp=False)` (Mangum) |
| REST | ✅ | ✅ |
| MCP `/mcp` | ✅ (Streamable HTTP) | ❌ 마운트 안 함 |
| Skill 입구 | ✅ (REST 호출) | ✅ (REST 호출) |
| `mcp` 의존성 | 포함 | 지연 import라 번들 제외 |

Lambda에서 MCP를 빼는 이유: Streamable HTTP의 스트리밍/상주성이 API Gateway+Lambda 제약과 맞지 않음.
MCP는 상주 서버(컨테이너)로만 서빙한다 (SERVERLESS.md 방침과 일치).

## 6. 테스트

- **MCP HTTP 인증:** `/mcp`에 Bearer로 도구 호출 시 요청별 신원 유도. 헤더 없으면 `unauthorized`.
  다른 토큰 → 다른 `spaces` 권한.
- **한 프로세스 통합:** `create_app(mount_mcp=True)`가 REST·`/mcp` 둘 다 서빙, 같은 스토어 공유
  (한 입구로 쓴 데이터가 다른 입구에서 보임).
- **Lambda 분리:** `create_app(mount_mcp=False)`엔 `/mcp` 없음(404), MCP import도 안 일어남.
- **회귀:** 기존 163개 테스트 전부 통과 유지.
- Skill은 문서라 자동 테스트 대상 아님 — curl 예시가 실제 엔드포인트와 맞는지 수동 확인.

## 7. 의존성

- `mcp`를 `[project].dependencies`로 승격 (서버 기본 기능). `create_app`의 `if mount_mcp:` 안에서
  지연 import → Lambda 번들 제외.
- `[serverless]` extra(mangum, boto3)는 그대로.

## 8. 문서 산출물 (구현 단계)

- **ADR 신설:** `docs/adr/NNNN-mcp-http-and-skill-dual-access.md` — "MCP stdio→HTTP 전환 + Skill(REST)
  이중 입구"를 되돌리기 어려운 결정으로 기록.
- **설계 문서 갱신:** `03-architecture.md`(C1 전송), `05-contracts.md`(stdio 경로 제거 반영),
  `README.md` / `SERVERLESS.md`.

## 9. 되돌리기 어려운 결정 요약

1. MCP 전송을 stdio → Streamable HTTP로 (계약 §3.1의 "HTTP 기본" 구현).
2. REST + MCP를 한 프로세스에서 서빙 (`mount_mcp` 플래그).
3. 비-MCP 환경용 Skill 입구를 REST 호출로 제공 (MCP 도구 6종과 동일 범위).
4. Lambda는 REST만 — MCP는 상주 서버 전용.
