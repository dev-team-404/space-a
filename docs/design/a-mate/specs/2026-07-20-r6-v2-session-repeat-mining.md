# R6 v2 — 세션 내 반복 지시·도구 시퀀스 마이닝 설계

- **날짜**: 2026-07-20
- **상태**: 초안 (리뷰 대기)
- **관련 문서**: [코칭 v3 설계](2026-07-19-coaching-v3-design.md),
  [R6 스킬 초안 설계](../../overview-mentor/specs/2026-07-19-r6-skill-draft-design.md)
- **선행 작업**: IDE 합성 블록(`<ide_opened_file>` 등) 프롬프트 오탐 수정 — 같은 PR에서 완료
  (`adapter.rs::is_synthetic_marker` + `user_version=1` 재수집 마이그레이션)

## 1. 문제

R6 v1은 **세션의 첫 프롬프트**가 정규화 동치일 때만 "반복 지시"로 판정한다.
그러나 실제 반복 작업은 세션 중간에 온다. 대표 시나리오:

> "PR 올린 후 항상 codex 리뷰를 시키고, 끝나면 PR에 달린 리뷰 코멘트와
> 종합 검토해서 조치하라고 반복적으로 명령한다."

이 패턴은 (a) 첫 프롬프트가 아니고, (b) 매번 표현이 조금씩 달라서 v1이 잡지 못한다.

## 2. 범위

| # | 작업 | 데이터 원천 | 신규 저장 |
|---|------|------------|----------|
| 1 | 도구 시퀀스 군집 — 신규 룰 (R23 제안) | 기존 `events`의 도구 호출 열 | 없음 |
| 2 | 세션 내 전체 프롬프트 축적 + R6 판정 확장 | JSONL user 라인 (기존 파싱 경로) | `prompt_events` 테이블 |

**비범위**: LLM 패러프레이즈 군집화(v3 후속), claude-code 외 에이전트 어댑터.

두 작업 모두 **감지는 SQL(수집 신호), 심화는 매칭 세션만 되짚기** 패턴을 유지한다
(`skill_draft::gather_context` 전례).

## 3. 작업 1 — 도구 시퀀스 군집 (R23)

프롬프트 원문 없이, 세션들이 공유하는 **도구 호출 시퀀스**로 반복 워크플로를 감지한다.

### 3.1 토큰화 (알파벳)

세션 내 도구 호출(`events` WHERE kind='tool_call', `source_offset` 순)을 토큰 열로 변환:

| tool_kind | 토큰 | 예 |
|-----------|------|----|
| bash | `bash:<명령 첫 단어>` (시크릿 리댁션 `<redacted:…>` target은 `bash`) | `bash:gh`, `bash:cargo` |
| mcp | `mcp:<server>` | `mcp:context7` |
| skill | `skill:<name>` | `skill:codex:rescue` |
| sub_agent | `agent` | `agent` |
| 파일/검색 내장 (read·edit·write·grep·glob) | `file-ops` | — |
| 미분류 도구 (`other`) | raw_name 소문자 (뭉개면 서로 다른 워크플로가 병합됨) | `todowrite` |
| 기타 내장 | tool_kind 그대로 | `web_fetch` |

연속 동일 토큰은 1개로 압축(RLE) — `file-ops` 연쇄가 시퀀스를 잠식하지 않게 한다.
사이드체인(서브에이전트) 도구 호출은 제외 — 사용자의 수동 워크플로가 아니다.

### 3.2 판정

- RLE 압축 열에서 길이 **3~6**의 n-gram 추출.
- 같은 `(host, n-gram)`이 **≥3 세션 / 14일** 등장하면 후보.
- 서로 포함 관계인 후보는 **최장 시퀀스만** 남긴다 — 단, 짧은 쪽의 등장 세션 수가
  **더 많으면** 독립 패턴으로 함께 유지한다 (짧고 강한 패턴을 길고 희소한 변형이
  지우면 안 됨 — PR#77 리뷰).
- **무의미 패턴 가드**: 토큰 다양성 ≥2, 그리고 **특이 토큰 ≥1 필수**.
  특이 토큰 = `skill:`/`mcp:`/`agent`, 또는 일반 명령 목록(`git`·`npm`·`npx`·`cargo`
  등 빌드·테스트·VCS·셸 유틸)에 없는 `bash:<명령>`. 일반 명령과 `file-ops`만으로 된
  시퀀스는 에이전트의 자율 루프(파일 수정 → 테스트 → 커밋)라 코칭 가치가 없다
  (2026-07-20 실사용 노이즈 `file-ops → bash:npx → bash:git` 판정 반영).

### 3.3 Finding

- `rule_id: "R23"` (R13~R22는 코칭 v3 예약 — §4.1, §11), `severity: Suggest`, `scope_kind: "pattern"`.
- `evidence`: `{ "sequence": ["bash:gh", "skill:codex:rescue", ...], "session_count": n, "window_days": 14 }`
  — 프롬프트 원문 없음. 허브 공유 화이트리스트 포함 여부는 추후 별도 검토(기본 제외 유지).
- `prescription`: `skillify` (payload에 sequence) → `skill_draft`가 시퀀스 매칭 세션들의
  프롬프트 표본·도구 통계를 모아 SKILL.md 초안 생성 (기존 흐름 확장).
- `dedup_key`: `R23|{host}|{시퀀스 해시8}`.

## 4. 작업 2 — `prompt_events` 축적 + R6 판정 확장

### 4.1 스키마

```sql
CREATE TABLE IF NOT EXISTS prompt_events (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  dedup_key TEXT UNIQUE NOT NULL,        -- uuid:offset (events와 동일 규칙)
  session_id TEXT NOT NULL, host TEXT NOT NULL, project_id TEXT NOT NULL,
  ts TEXT, source_file TEXT NOT NULL, source_offset INTEGER NOT NULL,
  norm60 TEXT NOT NULL,                  -- r6::normalize 결과 (그룹 키)
  preview TEXT NOT NULL                  -- 첫 줄 ≤120자 (기존 미리보기 원칙 그대로)
);
```

- 어댑터는 이미 모든 user 프롬프트에 `UserPrompt` 이벤트를 방출한다 — store가
  `sessions` 갱신(기존)에 더해 `prompt_events`에 INSERT OR IGNORE(신규).
- 사이드체인(서브에이전트) 프롬프트는 제외 — 사용자 지시가 아니다 (PR#77 Codex 리뷰).
- `sessions.first_prompt_preview`는 유지 (R10 대표 프롬프트, hub, session_ctx가 사용).

### 4.2 프라이버시

- `preview`는 기존 first_prompt와 **동일 원칙**: 첫 줄 120자 컷, 시크릿 패턴 시 미저장,
  합성 마커(`<ide_...>` 등) 제외. 추가 완화 없음.
- 로컬 SQLite 전용. 허브 공유 화이트리스트 제외 유지 (R6 v1과 동일 사유 — 원문 포함).
- 저장량: 프롬프트당 ~150B × 세션당 수십 건 → 연 수 MB 수준. 무시 가능.

### 4.3 R6 판정 변경

- 그룹 키: `(host, norm60)` — **첫 프롬프트 제한 제거**, `prompt_events` 전체 대상.
- 카운트: **DISTINCT session_id** (세션당 1회) — 한 세션에서 10번 반복한 것과
  10개 세션에서 1번씩 반복한 것을 구분해, 후자만 "세션을 여는 반복 지시"로 본다.
  세션 내 다회 등장 수는 evidence에 `occurrences_total`로만 병기.
- 문턱: `min_sessions=3 / 14일` 유지. 짧은 프롬프트(<8자) 제외 유지.
- `skill_draft::gather_context`: 매칭 기준을 `sessions_with_prompts`(첫 프롬프트)에서
  `prompt_events` 조회로 교체.

## 5. 마이그레이션

- `PRAGMA user_version 1 → 2`: 전체 재수집(events/sessions/ingest_state/daily_rollup 비움,
  findings·status 보존 — §4.4 전례). 기존 JSONL에서 `prompt_events` 백필이 목적.
- `→ 3`: R23 특이 토큰 가드 도입에 따른 기존 R23 finding 정리 (재산출).
- `→ 4`: prompt_events 사이드체인 제외 반영을 위한 전체 재수집.
- 신규 설치는 0→4 한 번에 통과 (중간 분기는 빈 DB에서 no-op).

## 6. 테스트 계획

| 대상 | 검증 |
|------|------|
| prompt_events 적재 | 첫/중간 프롬프트 모두 저장, dedup 멱등, 합성 마커·시크릿 제외 |
| R6 확장 | 중간 프롬프트 반복으로 발화, 세션당 1회 카운트, 문턱 미달 침묵 |
| R23 토큰화 | bash 첫 단어, RLE 압축, file-ops 묶음 |
| R23 판정 | 3세션 반복 발화, 포함 시퀀스 제거, 무의미 패턴 가드 |
| skill_draft | 시퀀스/프롬프트 매칭 세션의 재료 수집 |
| 마이그레이션 | 0/1→2 재수집, findings 보존, 멱등 |

## 7. 리뷰 포인트 (결정 필요)

| # | 결정 | 추천 | 대안 |
|---|------|------|------|
| 1 | `preview` 원문 저장 여부 | **원문 저장** — 기존 first_prompt와 동일 수준이고 skill_draft 표본 품질에 필요 | norm60 해시만 저장, 원문은 문턱 넘은 그룹만 |
| 2 | bash 토큰 세분도 | **첫 단어까지** (`bash:gh`) — 워크플로 식별에 필요, 시크릿 위험 없음(단어 1개) | `bash` 단일 토큰 |
| 3 | 신규 룰 번호 | **R23** (R13~R22는 코칭 v3 예약) | R6 하위 변형으로 통합 |
