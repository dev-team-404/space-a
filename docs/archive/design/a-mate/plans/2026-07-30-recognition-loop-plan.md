---
status: done
archived: 2026-08-03
---

# 인정 루프 (Recognition Loop) — 구현 계획

- **날짜**: 2026-07-30
- **컴포넌트**: a-mate (`crates/core`, `src-tauri`, `src/`)
- **스펙**: [PR #81 hub recognition loop](https://github.com/dev-team-404/space-a/pull/81) 설계안을 실행 계획으로 구체화
- **선행**: [지식 재사용 루프 닫기](../../common/specs/2026-07-25-close-knowledge-reuse-loop-design.md) (PR #110, 머지됨)

## 1. 무엇을 / 왜

가치가 **한 방향**으로만 흐른다.

```
a-mate(내 발견) ──► a-hub(팀 기록) ──► a-lens(조직이 관전)
                                        ↑ 여기서 끝 — 발견자에게 안 돌아옴
```

a-lens의 간판인 "지식 재사용 체인"을 **원 발견자만 모른다.** 인정 루프는 그 고리를 잇는다.

```
a-hub(누가 내 지식을 인용) ──► a-mate(로봇이 알려줌: "주인님 발견이 누군가를 구했어요")
```

**하나의 이벤트, 두 청중** — 같은 `ReuseEvent`가 a-lens에서는 *조직의 재사용 체인*으로,
a-mate에서는 *개인 칭찬*으로 렌더된다.

## 2. 실측으로 확정한 것 (스펙의 열린 질문 해소)

| 스펙 질문 | 실측 결과 (2026-07-30) |
|---|---|
| Q2: 발행 id == reuse-events id? | **동일.** 발행 시 `page_1` → `/reuse-events[].page_id == "page_1"`. 필드명은 `doc_id`가 아니라 **`page_id`** |
| a-mate가 내 발행분을 아는가? | **안다.** `hub_share_state(dedup_key, issue_id, page_id, shared_at)`에 저장 중 |
| 운영 배포 상태 | ⚠️ `GET /reuse-events` **404 (미배포)** — PR #110 머지 후 9일째. 재배포 필요 |
| 실제 인용 발생분 | 운영 이슈 74건 중 `knowledge_linked` 1건(초기 시드). **아직 실사용 인용 0건** |

→ 매칭은 **`hub_share_state.page_id` ↔ `/reuse-events[].page_id`** 대조뿐. 새 필드 불필요.

## 3. 설계 — 기존 인바운드 배관에 한 갈래 추가

방문·방명록이 쓰는 **커서 기반 폴링**을 그대로 따른다 (신규 아키텍처 없음).

| 기존 | 인정 루프 |
|---|---|
| `inbound::select_new_visits(rows, cursor)` | `inbound::select_new_reuses(rows, mine, cursor)` |
| 설정 `inbound_visits_cursor` | 설정 `inbound_reuse_cursor` |
| `pipeline::maybe_poll_inbound` | 같은 함수에 ③단계 추가 |
| `app.emit("life:visit")` | `app.emit("reuse:celebrated")` |
| `notices.ts` kind `visit` | kind `reuse` |
| `IdleContext.visits` | 일기 컨텍스트에 재사용 소재 |

### 커서 규약 (기존과 동일)

- 커서 `None`(첫 실행) = **emit 없이 초기화** — 설치 직후 과거분 도배 방지
- `created_at > cursor`인 것만 emit, 커서는 관측 최댓값으로 **단조 증가**
- → **1회만 축하**가 커서에서 자동 보장 (별도 dedup 불필요)

### 프라이버시 (스펙 §)

- 인용한 **팀(space)** 단위까지만 표현. 개인명 기본 off
- 상대 팀 **이슈 제목은 절대 싣지 않는다** (타 팀 업무 누출 방지)
- 수치는 사실만 — 추정치엔 `~` 라벨
- egress 없음 — **읽기 전용**. 이미 발행에 동의한 사용자만 해당

## 4. 작업 순서

| # | 작업 | 파일 | 검증 |
|---|---|---|---|
| 1 | `select_new_reuses` 순수 함수 | `core/inbound.rs` | 단위 테스트 (커서·필터·방어) |
| 2 | `ReuseNote` + 축하 문구 렌더 | `core/hub.rs` | 단위 테스트 (프라이버시·문구) |
| 3 | 내 발행분 page_id 조회 | `core/store.rs` | 단위 테스트 |
| 4 | 폴링 훅 + emit | `src-tauri/pipeline.rs` | E2E (로컬 허브) |
| 5 | 소식 `kind: 'reuse'` | `src/lib/notices.ts` | vitest |
| 6 | 일기 컨텍스트 반영 | `core/diary/mod.rs` | 단위 테스트 |

1~3은 **허브 배포와 무관하게** 완성 가능. 4는 로컬 허브로 E2E 검증한다.

## 5. 실패 무해

허브가 죽거나 `/reuse-events`가 없으면(**현재 운영 상태**) 조용히 건너뛴다.
404는 "구버전 허브"로 간주해 경고 1회만 남기고 앱 동작에 영향을 주지 않는다.
