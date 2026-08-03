---
status: done
archived: 2026-08-03
---

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
- **겹침 가족당 대표 1개**: **특이 토큰을 포함한** bigram을 공유하는 후보들을
  연결 요소(브리지 경유 포함)로 묶고, `score = 세션 수 × 길이`가 가장 높은 것만 남긴다.
  - 비특이 bigram(공통 빌드/파일 단계)만 겹치는 서로 다른 워크플로는 병합하지 않는다.
  - (주의: 부분 시퀀스의 세션 수는 항상 상위 시퀀스 이상이므로 "빈도 우위면 유지"
    방식은 dedup을 무력화한다 — 2026-07-20 카드 홍수 실사용 판정.)
- **host당 상한 5장** — 무관한 패턴이 아무리 많아도 코치 탭을 채우지 않는다.
- **밀려난 카드 정리**: 스캔마다 이번 평가에 없는 R23 활성('new') 카드는 내린다 —
  상한이 저장소에도 지켜지게. dismissed/resolved는 사용자 기록이라 보존.
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
- `→ 5`: R23 카드 홍수 정리 — 활성('new')만 삭제, dismissed/resolved는 보존.
- `→ 6`: 논리 dedup 키 도입(데이터 위생 스펙 §3) — 전체 재수집 + R6/R23 'new' 정화.
- 신규 설치는 0→6 한 번에 통과 (중간 분기는 빈 DB에서 no-op).

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

## 8. 캘리브레이션 — 스킬/커맨드 호출 프롬프트 제외 (2026-07-23)

### 8.1 오탐

코치 탭에 다음 카드가 반복 노출됐다:

> 💡 같은 지시를 3개 세션에서 반복했어요 —
> "feat/install-signal 브랜치에서 docs/superpowers/plans/2026-07-10-"
> 🧭 스킬(SKILL.md)로 묶으면 매번 다시 설명할 필요가 없어요

### 8.2 근본 원인 (실데이터 확인)

`prompt_events`를 만든 실제 프롬프트 3건(로컬 DB → WSL JSONL 원본 대조):

| 세션 | host | 프롬프트 |
|------|------|---------|
| `78ca3a79` | wsl:Ubuntu | `…플랜을 superpowers:subagent-driven-development 로 실행. 완료 후 PR.` |
| `d574816d` | wsl:Ubuntu | (동일) |
| `a846c9ea` | wsl:Ubuntu | `…Task 12–14만 superpowers:executing-plans로 실행해줘…` |

- 셋 다 `isSidechain=false`인 **진짜 사용자 프롬프트**다 — 사이드체인 누수(§4·마이그v4)가 아니다.
- 셋 다 **이미 스킬을 호출하는 지시**다. 브랜치·플랜문서·태스크 번호 등 인자만 바뀌는
  템플릿형 반복이라 `norm60`(60자 컷: `…브랜치에서 docs/superpowers/plans/2026-07-`)가
  거의 동일 → 느슨한 묶기로 한 후보가 되어 발화.
- R6이 "반복 지시 → 스킬로 묶어라"를 **스킬 호출에 다시 권하는** 순환 오탐.
  변하는 부분(브랜치·문서)은 스킬의 인자일 뿐 새 스킬로 코드화할 대상이 아니다.

### 8.3 조치

**어댑터가 raw 첫 줄에서 판정, 이벤트에 `is_command` 플래그로 전달**한다
(`adapter::is_command_invocation`). store는 `is_command`면 `prompt_events` 적재를
건너뛰되(R6 반복 마이닝 제외), `first_prompt_preview`(세션 대표)는 **보존**한다 —
사이드체인 제외와 같은 지점·같은 방식(§4). `EventKind::UserPrompt`에 `is_command: bool` 추가.

- **왜 어댑터인가 (검사 위치):** `preview`는 `extract_prompt_first_line`이 **첫 줄 120자**로
  자른 결과다. 스킬 토큰은 그 뒤에 오는 경우가 흔해(실측: windows-hook 프롬프트는 preview가
  정확히 120자에서 잘려 `superpowers:`를 잃음) `normalize(preview)`로는 못 잡는다. 그래서
  **자르기 전 raw 첫 줄**에서 판정한다.
- **왜 first_prompt는 보존인가 (스코프):** first_prompt는 hub 공유·다이어리·세션 상세의
  세션 대표 프롬프트다. 스킬 호출도 사용자가 직접 친 지시이므로 대표로는 유효 — R6 반복
  마이닝에서만 뺀다.
- **판정 규칙 (정밀도):** 스킬 참조 토큰 `<ns>:<name>`에서 **이름이 하이픈 포함 다단어**일
  때만 인정(`executing-plans`, `subagent-driven-development`). 이 제약이 도커 태그
  (`node:latest`)·git 참조(`origin:main`) 같은 단일단어 우변을 배제한다. `regex` 무의존
  (ASCII-safe 바이트 스캔). 슬래시 커맨드(`/codex:review` 등)는 별도 처리 불필요 —
  `<command-*>` 합성마커라 `is_synthetic_marker`가 이미 거른다.
- **잔여 한계:** `origin:feature-x`처럼 하이픈 브랜치를 콜론으로 쓴 git 참조는 오검출 가능
  (드물고 저위험 — 미제안 넛지 1건). 슬래시 아닌 순수 텍스트로 부른 단일단어 스킬
  (`codex:review`)은 미검출(실제로는 슬래시=합성마커 경로라 무관).
- 테스트: `adapter::is_command_invocation_precision`,
  `map_user_prompt_command_invocation_detected_past_preview_cut`(120자 뒤 토큰),
  `store::prompt_events_skip_command_invocations_but_keep_first_prompt`,
  `r6::r6_ignores_command_invocation_prompts`.
- 마이그레이션 `user_version → 8`: 기존 `prompt_events`에 남은 호출 프롬프트를 소급
  제거할 수 없어 전체 재수집(어댑터가 재수집 때 `is_command`로 제외) + R6 활성('new') 카드
  정화 (dismissed/resolved 보존, v6 전례).

> **경위:** 최초 구현은 `normalize()`에서 검사(슬래시 규칙 포함)했으나 Codex 리뷰가 3건
> 지적 — ① 콜론 토큰이 너무 관대(`node:latest` 오검출) ② `normalize`는 잘린 preview를 받아
> 120자 뒤 토큰을 놓침 ③ 선두 단일 경로(`/tmp …`)를 슬래시 커맨드로 오검출. 위 설계로 전부 반영.
