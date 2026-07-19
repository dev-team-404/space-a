---
status: done
archived: 2026-07-19
---

# x-api-key 전역 인증 설계

**날짜:** 2026-07-18
**상태:** 승인됨 (구현 예정)
**범위:** `a-hub/work` — 고정 공유키(x-api-key)를 전역 관문으로 추가. 기존 Bearer 신원은 그대로.

## 목표

기존 Bearer 토큰(사용자 신원) **위에** 고정 공유키 `x-api-key`를 추가한다.
"이 API를 부를 자격이 있는 클라이언트인가"를 게이트웨이 레벨에서 검사한다.
서버리스 배포 시 키를 환경변수로 선언한다.

## 인증 계층 구조

두 관문이 독립적으로 동작한다:

```
요청 → [1. x-api-key 미들웨어] → [2. Bearer 토큰 (엔드포인트별)] → 핸들러
         전역, 모든 요청            기존 방식, 신원(user_id)
         고정 공유키                각자 실패 시 각자 401
```

## 결정

| 항목 | 결정 | 근거 |
|------|------|------|
| 적용 범위 | 모든 요청에 전역(FastAPI 미들웨어) | 게이트웨이 앞단 |
| 공개 예외 | `/healthz`·`/readyz`만 면제 | LB·모니터링용. 나머지(discovery·spaces·guide)도 요구 |
| 미설정 시 | `SPACE_A_API_KEY` 없으면 검사 비활성 | 로컬·테스트 편의, 기존 `SPACE_A_TABLE` 방식과 일관 |
| MCP | `/mcp`도 요구 | 부모 앱 미들웨어가 마운트 서브앱까지 커버 |
| 비교 | `==` 단순 비교 | 데모 범위. 상수시간·rate limit은 범위 밖 |
| 서버리스 | SAM 파라미터 `ApiKey`(NoEcho) → env 주입 | 로그 노출 방지 |

## 변경 상세

### 1. 미들웨어 — `_auth.py` + `rest_server.py`

환경변수 `SPACE_A_API_KEY`. `create_app`에 `@app.middleware("http")` 등록:

```python
_EXEMPT = {"/healthz", "/readyz"}

@app.middleware("http")
async def _require_api_key(request, call_next):
    expected = os.environ.get("SPACE_A_API_KEY")
    if expected and request.url.path not in _EXEMPT:
        if request.headers.get("x-api-key") != expected:
            return UTF8JSONResponse(
                status_code=401,
                content={"error": {"code": "unauthorized",
                                   "message": "invalid or missing x-api-key"}},
            )
    return await call_next(request)
```

- `expected` 없으면 통과(검사 비활성).
- 면제 경로 정확 일치. `/mcp`는 면제 아님 → 키 요구.
- 에러 봉투·`UTF8JSONResponse` 재사용(charset·스키마 일관).

### 2. 서버리스 — `template.yaml`

```yaml
Parameters:
  ApiKey:
    Type: String
    NoEcho: true
    Default: ""        # 미설정이면 빈 값 → 검사 비활성
Resources:
  HubFunction:
    Properties:
      Environment:
        Variables:
          SPACE_A_TABLE: !Ref HubTable
          SPACE_A_API_KEY: !Ref ApiKey
```

배포: `sam deploy --parameter-overrides ApiKey=<키>` (또는 `--guided` → samconfig 저장).

### 3. 문서

- 스킬 `endpoints.md`·`SKILL.md`: 공통 헤더에 `x-api-key`(설정된 배포에서 필요) 안내.
- `SERVERLESS.md`: `ApiKey` 파라미터 + `x-api-key` 헤더 사용법.
- README 인증 절 갱신.

## 테스트 (TDD)

새 파일 `tests/test_api_key.py` (`monkeypatch.setenv`로 env 제어):

1. env 미설정 → 키 없이도 200
2. env 설정 + 올바른 키 → 통과
3. env 설정 + 틀린/없는 키 → 401, 에러 봉투 형식
4. env 설정이어도 `/healthz`·`/readyz`는 키 없이 200
5. env 설정 시 공개 엔드포인트(`GET /spaces`)도 키 요구 → 키 없으면 401

기존 테스트는 env 미설정이라 검사 비활성 → 회귀 없음.

## 범위 밖

- 키 로테이션·다중 키·per-client 키
- 상수시간 비교, rate limiting
- API Gateway 자체 API Key/Usage Plan
