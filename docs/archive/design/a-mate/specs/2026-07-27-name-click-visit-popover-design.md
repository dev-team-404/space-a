---
status: done
archived: 2026-07-27
---

# G4 — 이름 클릭 → 그 사람 홈 이동 팝오버 설계

- **날짜**: 2026-07-27
- **컴포넌트**: a-mate (프론트 단독 — 서버·Rust 미접촉)
- **로드맵**: [2026-07-26-life-social-diary-followups-roadmap.md](../../../../design/a-mate/plans/2026-07-26-life-social-diary-followups-roadmap.md) 2차 배치 G4 (백로그 5)
- **선행**: G1(작성자 표기, PR #106) · G2(답글 스레드, PR #107) · G3(봇 자동 답글, PR #108) · G5(답글 품질·ADR 0022, PR #111) — 전부 main 머지 완료

## 1. 요구

방명록·방 화면에서 **사람 이름을 클릭**하면 그 사람의 Life 방으로 이동하는 **팝오버 버튼**을 띄운다.

### 확정 결정 (브레인스토밍 2026-07-27)

| 결정 | 선택 | 근거 |
|---|---|---|
| 적용 범위 | **방명록 작성자(원글·답글) + 방 점유자 라벨** | 이름이 렌더되는 2대 지점. 로드맵 문구("방명록/방 등") 일치. 설정>일촌 목록은 문맥 이탈이 커서 제외 |
| 클릭 동작 | **팝오버 버튼** ("○○네 놀러가기") | 오클릭 오이동 방지. 백로그 문구("팝업 버튼") 일치 |
| 이동 불가 케이스 | **이동 가능할 때만 클릭 가능** | 대상 life가 있고 현재 방과 다를 때만 클릭 어포던스. 그 외엔 기존과 동일한 일반 텍스트 — 죽은 팝오버 없음 |
| 구현 방식 | **순수 모듈 + 공용 컴포넌트** | 판별 로직은 `people.ts`(Vitest 대상), UI는 `NameChip.svelte` 하나로 두 지점 재사용. `showOwnerAvatar`(guestbook.ts) 선례 |

## 2. 매핑 경로 (조사 확정)

`author_agent_id → life_id` 매핑은 **기존 API로 충분** — 서버 확장 불필요.

| 소스 | 제공 값 | 비고 |
|---|---|---|
| `lifePeople()` (`api.ts:214`) | `{ agent_id, name, life_id, is_friend }[]` | **나를 제외한** 전체 등록자 (`a-hub life.py:259-265`) |
| `lifeView().me` | `agent_id`(=meId), `my_life_id` | 내 자신 판별·내 방 타겟 |
| `GuestbookEntry.author_agent_id` | 작성자 agent_id | G2에서 도입. 봇 답글(G3)은 방 주인 토큰으로 작성 → author_agent_id = 방 주인 |
| `LifeOccupant.agent_id` | 점유자 agent_id | LifeView 라벨에 이미 사용 |

## 3. 데이터 흐름

```
이름 렌더 지점                판별 컨텍스트                          이동
GuestbookTab ──┐   ┌─ meId · myLifeId · currentLifeId ─┐
 (원글·답글)   ├─→ │ + people (getPeople: TTL 캐시)     ├─→ resolveVisitTarget() ─→ 팝오버 ─→ lifeGoto(lifeId)
LifeView ──────┘   └───────────────────────────────────┘
 (점유자 라벨)
```

이동 후 화면 전환은 기존 폴링이 처리한다 — App.svelte(2초 tick)가 방 변경을 감지해 방문 모드로 전환하고, LifeView도 자체 폴링으로 갱신한다. **추가 배선 없음** (Mascot `gotoLife` 선례).

## 4. 판별 로직 — 신규 순수 모듈 `src/lib/people.ts`

```ts
export interface VisitTarget { lifeId: string; label: string }
export interface VisitCtx {
  meId: string; myLifeId: string; currentLifeId: string;
  people: Pick<LifePerson, 'agent_id' | 'life_id'>[];
}
export function resolveVisitTarget(agentId: string, displayName: string, ctx: VisitCtx): VisitTarget | null
```

| 순서 | 조건 | 결과 |
|---|---|---|
| 1 | `agentId === meId` 이고 `myLifeId === currentLifeId` | `null` — 내 방에서 내 이름(내 봇 답글 포함) |
| 2 | `agentId === meId` 이고 방문 중 | `{ lifeId: myLifeId, label: '내 방으로 돌아가기' }` |
| 3 | `people`에 `agent_id` 없음 | `null` — 탈퇴·미등록 |
| 4 | `person.life_id === currentLifeId` | `null` — 지금 보는 방 주인(그 방 봇 답글 행이 자동으로 해당) |
| 5 | 그 외 | `{ lifeId: person.life_id, label: '{displayName}네 놀러가기' }` |

- `displayName`은 **클릭된 표시 이름 그대로**(방명록=`author_name`, 점유자=`o.name`) — 화면 표기와 팝오버 라벨이 항상 일치. ADR 0022(봇 라벨=봇 이름만) 이후 라벨 문자열로 사람/봇을 구분하지 않으며, 구분할 필요도 없다(판별은 전부 agent_id 기준).
- 빈 `meId`/`myLifeId`/`currentLifeId`(로딩 전)면 `null` — 컨텍스트 미비 시 안전하게 비활성.

### people 조회 — `getPeople()`

- `lifePeople()` 래퍼. **모듈 레벨 TTL 캐시(10초) + in-flight 공유** (`api.ts` `lifeViewCache` 500ms 선례를 완화한 값 — 등록자 목록은 저빈도 변경).
- 실패(허브 순단·미연결) 시 `[]` 반환 → 모든 이름이 일반 텍스트로 강등. **기능만 조용히 꺼지고 화면은 깨지지 않는다.**
- 테스트를 위해 **fetcher 주입**으로 확정: `getPeople(fetcher)` — fetcher는 필수 인자. `people.ts`는 `guestbook.ts`처럼 type-only import만 갖는 순수 모듈로 유지하고(런타임에 `api.ts`를 끌지 않음), 실코드는 NameChip이 `lifePeople`을 넘긴다. 테스트는 가짜 fetcher 주입 — `vi.mock` 불필요.

## 5. UI — 신규 공용 컴포넌트 `src/lib/ui/NameChip.svelte`

- **Props**: `agentId: string`, `name: string`, `meId: string`, `myLifeId: string`, `currentLifeId: string`.
- 마운트/props 변경 시 `getPeople()` → `resolveVisitTarget()` 재계산.
- **타겟 없음(null)**: 기존과 동일한 일반 텍스트(`<b>{name}</b>` 상당) — 클릭 어포던스 없음.
- **타겟 있음**: 이름을 버튼으로 렌더(점선 밑줄 + 포인터 커서). 클릭 → 이름 옆 소형 팝오버에 `[{label} →]` 버튼 1개.
  - 바깥 클릭으로 닫힘 — 투명 백드롭 버튼 (LifeView `.menu-dismiss` 선례).
  - 버튼 클릭 → `lifeGoto(lifeId)` → 팝오버 닫기. `busy` 가드로 중복 호출 방지.
  - 이동 실패는 조용히 무시하고 팝오버만 닫는다 (Mascot `gotoLife` 선례).
- LifeView의 점유자 라벨은 z-index 스택(30+) 위에 있으므로 팝오버는 충분히 높은 z-index로.
- people 도착 전엔 일반 텍스트였다가 도착 후 클릭 가능으로 바뀌는 pop-in은 허용(수 백 ms).

## 6. 적용 지점

| 파일 | 변경 |
|---|---|
| `src/lib/ui/GuestbookTab.svelte` | 원글 `<b>{t.entry.author_name}</b>`·답글 `<b>{reply.author_name}</b>` → `NameChip` (2곳). props에 `myLifeId` 추가. 아바타(G5)·시간·삭제 버튼 등 주변 미접촉 |
| `src/App.svelte` | `<GuestbookTab …>`(195행)에 `myLifeId` 전달 (이미 보유한 상태값) |
| `src/lib/ui/LifeView.svelte` | 점유자 라벨 `<span>{o.name}</span>`(139행) → `NameChip` (`me.agent_id`·`me.my_life_id`·`me.life_id` 보유) |

## 7. 엣지 케이스

| 케이스 | 동작 |
|---|---|
| 내 방에서 내 이름/내 봇 답글 | 일반 텍스트 (규칙 1) |
| 남의 방에서 내 이름 클릭 | "내 방으로 돌아가기" (규칙 2) |
| 봇 답글 행 (author = 그 방 주인) | 그 방을 보는 중이면 일반 텍스트 (규칙 4), 다른 방 방명록에 남은 것이면 이동 가능 (규칙 5) |
| 탈퇴·미등록 agent | 일반 텍스트 (규칙 3) |
| hub 순단 / people 조회 실패 | `[]` → 전부 일반 텍스트, 화면 무해 |
| 이동 중 재클릭 | busy 가드 |
| 로딩 전(컨텍스트 빈 문자열) | `null` → 일반 텍스트 |

## 8. 테스트 (TDD — Vitest, 순수 모듈만)

- `src/lib/people.test.ts` (co-location 선례): §4 규칙 1~5 각각 + 라벨 조립("○○네 놀러가기"/"내 방으로 돌아가기") + 컨텍스트 미비 시 null + `getPeople` TTL 캐시(TTL 내 fetch 1회)·실패 시 `[]`.
- Svelte 컴포넌트 렌더 테스트는 인프라 없음(신설 금지) → NameChip은 얇게 유지하고 로직은 전부 `people.ts`에 둔다.
- 권위 테스트: 네이티브 Windows PowerShell `npm test`. Rust 미접촉이므로 `cargo test`는 회귀 확인용.

## 9. 비범위·판단

- **서버(a-hub)·Rust(src-tauri, crates/core) 미접촉.**
- 설정>일촌 목록(PrivacyGroup)·Mascot 방 목록 메뉴(이미 이동 메뉴)는 범위 외.
- **ADR 불필요**: 프론트 단독·가역적 결정 (전례 ADR 0020~0022는 표기 규범·서버 계약이었음).
- 실환경(허브 연결) 확인 1회 필요: 사외망 개발 PC에서는 hub 도달 불가 → 이동 동작 스모크는 사용자 환경에서.
