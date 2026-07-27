# 자동 방명록 (P3) + 방문자 사람/봇 판별 (G5 이월) 설계

- **날짜**: 2026-07-27
- **컴포넌트**: a-mate (+ a-hub 소규모 — `author_kind` 플래그)
- **관계**:
  - 로드맵 [2026-07-26-life-social-diary-followups-roadmap.md](../plans/2026-07-26-life-social-diary-followups-roadmap.md)의 **P3** + **G5 이월(사람/봇 판별)** + 묶음 실행 계획 **③**.
  - G3/G5 선례([아카이브: 봇 자동 답글](../../../archive/design/a-mate/specs/2026-07-27-guestbook-auto-reply-design.md)·[답글 품질 개선](../../../archive/design/a-mate/specs/2026-07-27-guestbook-reply-quality-design.md) — `build_guestbook_reply_prompt`·`compute_guestbook_reply`·`maybe_reply_guestbook`)의 프롬프트 조립·주입 방어·실패 무해 패턴을 벤치마킹.
  - 작성자 표기는 [ADR 0022](../../../adr/0022-guestbook-bot-reply-label.md)(봇 이름만), 답글 권한은 [ADR 0021](../../../adr/0021-guestbook-replies-one-depth.md)을 따른다.

## 1. 요구사항 (브레인스토밍 확정)

내가 남의 방을 방문(`life_goto`)하면 내 봇이 봇 성향(MBTI + 호칭 + 페르소나)으로 방명록 문구 1개를 남긴다. 단 "기계적 도장"이 아니라 **방문의 이유**가 있을 때만 남기고, 실패는 무해하게 조용히 넘어간다.

| 결정 항목 | 확정안 |
|---|---|
| 트리거 | `life_goto`(수동 방 이동)만. 자기 방(`life_id == hub_life_id`)은 skip. 앱 시작 시 자동 입장(`lib.rs`)은 항상 자기 방이라 구조적으로 미접촉 |
| 게시 여부 | 설정 토글 `visit_guestbook_enabled` (미설정 = **on**) |
| 도배 방지 바닥 | 같은 방에 내 top-level 글이 **24시간 이내** 있으면 무조건 skip — **서버 원천 판정**(대상 방 방명록 GET, 로컬 상태 없음. G3 "서버 데이터가 dedup의 원천" 철학) |
| 이유 게이트 | **첫 방문**(그 방에 내 top-level 글 없음) = 무조건 남김. **재방문** = ①주말 ②평일인데 내 주인 한가(`OwnerVibe::Idle`) ③마지막 내 글 ≥7일(오랜만) 중 하나일 때만. 이유는 문구에도 반영 |
| 실패 처리 | 엔진 미설정 = no-op, 모든 실패(GET/생성/POST) = warn 로그 후 skip. 정적 폴백 문구 없음 |
| 사람/봇 판별 | 서버 플래그 **`author_kind`**(`"human"` \| `"bot"`, 선택 필드, 없으면 human 간주). 이름 휴리스틱은 ADR 0022(봇 이름만) 이후 불가능 — 채택 안 함 |
| 재사용 | 방명록 작성 로직은 커맨드 인라인이 아닌 **재사용 함수**로 분리 — 미래 자율 방문 기능이 그대로 호출 |

### 스코프 제외 (후속 편입)

- **자율 방문**(휴일/한가할 때 봇이 스스로 일촌 방에 놀러가 무조건 방명록) — 방 선택·체류/귀가·아바타 이동 부작용 등 자체 설계 필요 → **로드맵 묶음 ②에 편입 기록** (본 세션 완료 시 로드맵 갱신).
- **놀러갔다 온 일기 반영** — P1(방문 경험→일기)의 정의 그대로 → **묶음 ②에 편입 기록**. 본 세션은 diary 프롬프트 미접촉.
- 방 인테리어 인식(P2), 상대 근황·일기 참조(P1), UI 봇 배지(장차 `author_kind`로 가능).

## 2. 아키텍처

### 컴포넌트 배치

| 위치 | 변경 | 내용 |
|---|---|---|
| **a-hub** `life.py` + `store.py` | 소규모 | guestbook POST body에 선택 필드 `author_kind` 수용("human"/"bot" 외 값은 400), 저장, GET 응답에 에코. 필드 없는 기존 행·구클라 요청 = human 간주. contracts/ 무변경(life API는 계약 범위 밖 — 조사로 확인) |
| **core 신규** `crates/core/src/visit.rs` | 신규 모듈 | 순수 로직 전부 (아래 §3·§4). UI·store 비의존, MockEngine·고정 입력으로 결정적 단위 테스트 |
| **src-tauri 신규** `src-tauri/src/visit.rs` | 신규 모듈 | 글루 `maybe_sign_guestbook(...)` — 설정 스냅샷(락)→즉시 해제→네트워크·LLM 락 밖 (G3 `maybe_reply_guestbook` 규율). **자율 방문의 재사용 진입점** |
| `src-tauri/src/commands.rs` | 소폭 | `life_goto` 성공 시 `std::thread::spawn`으로 글루 호출. `life_add_guestbook`(사람 수동 경로)에 `author_kind="human"` 명시 |
| `crates/core/src/life_client.rs` | 소폭 | `add_guestbook(life_id, body, author_name, parent_id, author_kind)` — 기존 호출처 갱신(G3 답글=`"bot"`, 수동=`"human"`) |
| `crates/core/src/mascot.rs` | 소폭 | `ReplyTarget`에 `author_kind` 추가 + `select_reply_targets` 파싱. ①세션 접촉부(`compute_chatter_pool`)와 다른 영역 |
| `src-tauri/src/pipeline.rs` | 1줄대 | G3 답글의 봇 안부 게이트: `ask_about_bot = should_ask_about_bot(entry_id) && author_kind != bot` |
| **프론트** 설정 UI + `api.ts` | 소폭 | 토글 "방문 시 봇 방명록 남기기"(`visit_guestbook_enabled`) + `GuestbookEntry.author_kind?: 'human'\|'bot'` 타입 |

### 데이터 흐름

```
life_goto(life_id) ── client.enter 성공 ──▶ 즉시 반환 (방 이동 UX 무지연)
        └─ std::thread::spawn ▶ maybe_sign_guestbook(store, life_id)
             ① 게이트: 토글 off / 자기 방 / hub 미연결 / 엔진 없음 → 조용히 종료
             ② client.guestbook(life_id) GET → visit_sign_decision(...)
                 · 24h 이내 내 top-level 글 → skip
                 · 내 글 없음 → Some(FirstVisit)
                 · 그 외 → Weekend / OwnerIdle / LongTimeNoSee(≥7일) or None(skip)
             ③ client.life_state(life_id) → 방 주인 이름 (실패 시 이름 없이 진행)
             ④ compute_visit_guestbook(engine, ...) — LLM 한 줄 생성
             ⑤ client.add_guestbook(..., author_kind="bot") — 실패는 warn+skip
```

②의 GET이 실패하면 **보수적으로 skip**(판정 불가 상태에서 게시하지 않음). ⑤에서 구서버(`author_kind` 미배포)는 모르는 필드를 무시하므로 게시 자체는 성공 — human으로 보일 뿐 무해라 호환 게이트(G3의 parent_id 에코 검증류) 불필요.

## 3. 판정 로직 (`visit_sign_decision`)

```rust
pub enum SignReason { FirstVisit, Weekend, OwnerIdle, LongTimeNoSee }

pub fn visit_sign_decision(
    entries: &[serde_json::Value], // 대상 방 방명록 (서버 최신순)
    my_agent_id: &str,
    now: chrono::DateTime<chrono::Utc>,
    is_weekend: bool,              // 달력 기준 (chrono weekday)
    vibe: OwnerVibe,               // 내 주인 활동 vibe (mascot::owner_vibe 재사용)
) -> Option<SignReason>
```

- 내 top-level 글(`author_agent_id == my_agent_id && parent_id 없음`)의 최신 `created_at`을 찾는다. 파싱 불가/필드 누락 행은 방어적으로 무시.
- 없음 → `Some(FirstVisit)`.
- 최신 글이 24h 이내 → `None` (도배 바닥 — 이유가 있어도 skip).
- 24h 초과 시 우선순위: `LongTimeNoSee`(≥7일) > `Weekend` > `OwnerIdle`(주말 아닐 때만) > `None`.
  - 오랜만이 최우선인 이유: 문구 소재로 가장 구체적("오랜만에 들렀어요")이고, 주말·한가함은 그날 내내 참이라 변별력이 낮다.
- 자기 방 제외·토글은 이 함수 밖(글루)에서 — 판정 함수는 순수하게 유지.

## 4. 프롬프트 설계 (P2 경계 준수)

G5 답글 프롬프트(`build_guestbook_reply_prompt`)의 골격을 방문판으로 변주한다:

- **`build_visit_guestbook_prompt(honorific, mbti, reason: SignReason)`** (system):
  - 역할: "{honorific}의 마스코트가 {honorific} 대신 이웃 방에 들러 남기는 방명록 인사".
  - 어투: 방 주인에게 **존댓말**(능청·위트 유지), 내 주인은 **3인칭**(G5 규범 동일). `mbti_voice_hint` + `voice_guidance()` 주입.
  - 이유 한 스푼: FirstVisit="처음 들러서 인사", Weekend="주말이라 놀러 옴", OwnerIdle="{honorific}이 요새 한가해서 심심해서 옴", LongTimeNoSee="오랜만에 들름" — **이 사실만** 쓰고 다른 근황·수치·감정 지어내기 금지 (G5 fabrication 억제 동일).
  - 출력: 한 줄(100자 이내), 번호·불릿·따옴표 금지.
- **`build_visit_user_msg(room_owner_name: Option<&str>)`** (user): 방 주인 이름은 서버/타인 통제 값 → system이 아닌 user 메시지로 분리(G3 주입 방어 선례). 이름 없으면 "이웃"으로.
- **`compute_visit_guestbook(engine, honorific, mbti, reason, room_owner_name) -> Result<String>`**: 빈 출력 = Err(호출자 warn+skip), 500자 방어 truncate (`compute_guestbook_reply` 동일).
- **작성자 표기**: `bot_author_name(user_name)` 재사용 — 봇 이름만 (ADR 0022). 남의 방이라 소유 맥락이 없지만, 방문 인사 본문이 "{honorific} 대신 왔다"는 맥락을 자연히 담으므로 표기 규범은 유지한다.

**안 넣는 것**: 방 인테리어(P2), 상대 일기·근황(P1), `facts_block`/원시 수치(fabrication 억제), 상대 방명록의 다른 글 내용.

## 5. `author_kind` 스키마 (a-hub)

| 항목 | 내용 |
|---|---|
| POST `/life/{id}/guestbook` body | `author_kind`: 선택, `"human"` 또는 `"bot"`. 그 외 값 400. 생략 시 저장값 없음(=human 간주) |
| GET 응답 entry | `author_kind` 에코 (없는 기존 행은 필드 생략 또는 null) |
| DB | guestbook 테이블에 nullable 컬럼 추가 (`store.py` 마이그레이션 — G2 `parent_id` 선례) |
| 의미 | "이 글을 봇이 자동 작성했는가". 사람이 UI로 쓴 글=human, P3 방문 인사·G3 자동 답글=bot |
| 하위호환 | 필드 없음=human이 정확한 이유: 봇 top-level 글은 P3가 최초이고 P3는 항상 플래그를 보냄. G3 답글은 top-level이 아니라 판별 소비처(아래) 밖 |
| 소비처 (이번 스코프) | G3 답글의 봇 안부 질문 게이트 — 원글 author_kind=bot이면 `ask_about_bot=false` ("당신의 봇은 잘 지내나요?"를 봇에게 묻는 어색함 방지) |

봇끼리의 상호작용은 유한하다: A봇의 방문 인사(top-level) → B봇 답글 1개(글당 1회, 답글은 reply 대상이 아님) — 루프 없음.

## 6. 에러 처리·안전 요약

| 상황 | 처리 |
|---|---|
| 토글 off / 자기 방 / hub 미연결 / 엔진 미설정 | 조용히 no-op |
| 방명록 GET 실패 (쿨다운 판정 불가) | 보수적 skip + warn |
| life_state 실패 (주인 이름 없음) | 이름 없이 진행 ("이웃") |
| LLM 생성 실패·빈 출력 | warn + skip |
| POST 실패 | warn + skip (재시도 없음 — 다음 방문에서 자연 재시도) |
| 구서버 (author_kind 미지원) | 게시는 성공, human으로 보일 뿐 — 무해 |
| 방 주인 이름·방명록 내용의 주입 시도 | 신뢰불가 값은 user 메시지 격리 + system에 "지어내기 금지" (G3/G5 동일) |

## 7. 테스트 전략 (TDD)

| 대상 | 테스트 |
|---|---|
| `visit_sign_decision` | 첫 방문 / 24h 경계(직전·직후) / 이유 우선순위(오랜만>주말>한가) / 평일+Idle / 이유 없음 skip / 잘못된 행 방어 / 내 답글(parent 있는 글)은 무시 |
| 프롬프트 조립 | 존댓말·3인칭 지시 포함 / reason별 문구 / MBTI voice 주입 / 주입 방어 골격 / user msg 분리 |
| `compute_visit_guestbook` | MockEngine 정상 한 줄 / 빈 출력 Err / 500자 truncate |
| `bot_author_name`·`select_reply_targets` | author_kind 파싱(bot/human/누락) — 기존 테스트 확장 |
| a-hub | `author_kind` 저장·에코·400 검증·생략 시 하위호환 (pytest, G2 선례) |
| 프론트 | 토글 저장·로드 (기존 설정 패턴 따름) |
| 실환경 | 팀 life 서버 스모크(방문→방명록 확인, 봇끼리 답글)는 **PR 체크리스트로 사용자 몫** |

## 8. 미래 확장 지점 (참고)

- **자율 방문(묶음 ② 편입 예정)**: 파이프라인에서 이유 발생 시 일촌 방 선택 → `client.enter` → `maybe_sign_guestbook`(본 스펙의 글루 그대로) → 귀가. 쿨다운·이유 게이트가 함수 안에 있어 도배 방지 자동 적용. 자율 방문은 "무조건 남김" 요구이므로 글루에 `force_reason` 같은 진입 인자가 필요할 수 있다 — 그때 설계.
- **UI 봇 배지**: `author_kind`가 이미 내려오므로 `GuestbookTab.svelte`에서 표시만 추가하면 됨.
