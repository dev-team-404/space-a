# a-mate 주인 메모리(Owner Memory) 설계

- **날짜**: 2026-07-22
- **컴포넌트**: a-mate (Pillar 1)
- **범위**: 마스코트가 주인이 말한 자유 텍스트 사실을 기억하는 기능. 채팅 중 LLM `save_memory` 툴콜로 명시적 저장, 전용 UI에서 조회·편집·삭제, 활성 메모리를 채팅·코칭·일기 시스템 프롬프트에 결정론적으로 주입. **범위 밖**: 분류/태그, 만료(TTL), 자동 추출, 채팅으로 잊기, 관련도 랭킹.

## 배경 / 문제

현재 a-mate에는 "주인이 말한 것을 기억하는" 메모리 기능이 없다. 존재하는 것은 고정 프로필 4필드(name/org/uuid/mbti, `settings` 테이블)와 로그에서 매 스캔 재계산되는 역량 프로필뿐이며, 채팅은 창 수명 동안만 프론트 메모리에 남고 **의도적으로 영속화하지 않는다**(`chat-store.svelte.ts`). 시스템 프롬프트는 트랜스크립트 원문 없이 파생 수치·advice 요약만으로 결정론 조립된다(전송 경계).

주인은 마스코트가 자신에 대한 사실("나는 비간을 먹지 않아", "목요일 오후는 회의")을 기억하고 대화·코칭·일기에서 자연스럽게 참조하기를 원한다. 이를 위해 **사용자 큐레이션 기반의 영속 메모리**를 도입한다.

## 결정 요약

| 항목 | 결정 | 비고 |
|------|------|------|
| 저장 트리거 | 명시적 (채팅 LLM 툴콜) + 수동 편집 | 자동 추출 없음 |
| 캡처 구현 | 네이티브 tool-calling 루프 (`save_memory`) | on-prem 미지원 시 graceful degrade |
| 회상 범위 | 채팅 · 코칭 · 일기 프롬프트 3곳 주입 | 결정론 조립, LLM 랭킹 없음 |
| 메모리 형태 | 자유 텍스트 1건 + 생성시각 | 분류/태그·TTL 없음 |
| 수명 | 삭제 전까지 영구 (하드 삭제) | 수동 UI에서 관리 |
| 저장 위치 | 신규 `memories` 테이블 | key-value `settings` 아님 |
| 관리 UI | `LifeSettingsTab.svelte`의 "주인 메모리" 카드 | 전용 탭 아님 |
| 전송 경계 | 메모리 텍스트는 엔진으로 전송 | 파생 수치 전용 원칙을 메모리에 한해 완화 |

## 아키텍처 / 컴포넌트

| # | 컴포넌트 | 변경/추가 | 위치 |
|---|----------|-----------|------|
| 1 | `memories` 테이블 + Store CRUD | 신규 테이블 + `add/list/update/delete/count_memory` | `crates/core/src/store.rs` |
| 2 | 메모리 회상 헬퍼 | `memory_block(store) -> String` (캡 적용) | `crates/core/src/` (신규 `memory.rs` 또는 `chat.rs`) |
| 3 | 프롬프트 주입 | `ChatContext`·`CoachingBrief`·diary 프롬프트에 메모리 블록 | `crates/core/src/chat.rs`, `diary/mod.rs` |
| 4 | Engine tool-calling | `chat_with_tools(...) -> ChatTurn` + OpenAI `tools` 파싱 | `crates/core/src/diary/engine.rs` |
| 5 | `chat_send` tool-loop | 툴콜→저장→재호출 루프 + graceful degrade | `src-tauri/src/commands.rs:460` |
| 6 | 관리 커맨드 | `memory_list/add/update/delete` Tauri 커맨드 | `src-tauri/src/commands.rs` |
| 7 | 프론트 API + UI | `api.ts` 래퍼 + "주인 메모리" 카드 | `src/lib/api.ts`, `src/lib/ui/LifeSettingsTab.svelte` |

## 상세 설계

### 1. 데이터 모델

리스트 + 행별 식별자·시각이 필요하므로 key-value `settings`가 아닌 전용 테이블을 기존 스키마 배치(`CREATE TABLE IF NOT EXISTS ...`)에 추가한다.

```sql
CREATE TABLE IF NOT EXISTS memories (
  id         INTEGER PRIMARY KEY AUTOINCREMENT,
  text       TEXT NOT NULL,
  created_at TEXT NOT NULL,                 -- 로컬 날짜 또는 RFC3339
  updated_at TEXT,                          -- 편집 시각 (없으면 NULL)
  source     TEXT NOT NULL DEFAULT 'chat'   -- 'chat' | 'manual'
);
```

Store 메서드:

| 메서드 | 동작 |
|--------|------|
| `add_memory(text, source) -> i64` | 신규 행 삽입, id 반환 |
| `list_memories() -> Vec<Memory>` | `created_at ASC, id ASC` (안정적 주입 순서) |
| `update_memory(id, text)` | text·updated_at 갱신 |
| `delete_memory(id)` | 하드 삭제 |
| `count_memories() -> u64` | 총 개수 |

빈/공백 `text`는 저장 거부(트리밍 후 비면 에러). 마이그레이션은 `CREATE TABLE IF NOT EXISTS`만으로 충분 — 기존 데이터에 영향 없음.

### 2. 회상 (프롬프트 주입) — 결정론 조립

공유 헬퍼 `memory_block(store) -> String`:

- `list_memories()`를 불릿(`- {text}`)으로 직렬화.
- **캡**: 최대 40개 **또는** 누적 ~2000자 초과 시 **최신 유지**하고 초과분은 버리며 `log::info`로 드롭 수를 남긴다(사용자 큐레이션이라 도달 가능성 낮음, 무음 절단 금지).
- 메모리가 없으면 빈 문자열 반환 → 프롬프트에 블록 미포함.

주입 지점과 문구:

| 프롬프트 | 변경 |
|----------|------|
| `build_chat_system_prompt` | `ChatContext`에 `memories: Vec<String>` 추가 |
| `build_coaching_system_prompt` | `CoachingBrief`에 `memories: Vec<String>` 추가 |
| diary `build_system_prompt` / `build_idle_prompt` | 메모리 블록 인자 추가 |

블록 형식(예):

```
[주인에 대해 기억한 것]
- 주인은 비간을 먹지 않음
- 목요일 오후는 회의로 바쁨
```

동반 지시문: "이 사실들을 관련될 때만 자연스럽게 언급하고 매번 나열하지 마세요. 여기 없는 사실은 지어내지 마세요." — 기존 "정밀도의 선" 지시와 일관.

### 3. 캡처 — tool-calling 루프

**Engine trait 확장** (`engine.rs`):

- 신규 타입: `ToolDef { name, description, parameters: serde_json::Value }`, `ToolCall { id, name, arguments: String }`, `ChatTurn { text: Option<String>, tool_calls: Vec<ToolCall>, tokens_used: u64 }`.
- 신규 메서드: `fn chat_with_tools(&self, system: &str, messages: &[ChatMessage], tools: &[ToolDef]) -> Result<ChatTurn>`. 기존 `chat`은 유지(툴 불필요 경로).
- `ChatMessage` 확장: 어시스턴트의 tool_call 에코와 `role:"tool"` 결과(`tool_call_id`)를 실을 수 있도록 선택 필드 추가(정확한 형태는 구현 계획에서 확정).
- `OpenAiCompatEngine`: 요청 본문에 `tools` + `tool_choice:"auto"` 추가, 응답 `choices[0].message.tool_calls` 파싱. 툴콜 없으면 `ChatTurn.text`만 채움.
- `MockEngine`: 테스트용 캔드 툴콜을 1회 낸 뒤 텍스트를 반환하도록 설정 가능(결정론 테스트).

**`chat_send` 루프** (`commands.rs:460`):

1. 기존대로 의도 분류 → 시스템 프롬프트 구성(**메모리 블록 포함**).
2. `save_memory { text: string }` 툴 정의를 준비.
3. **최대 3회** 루프:
   - `engine.chat_with_tools(system, messages, [save_memory])` 호출.
   - `tool_calls`가 있으면: 각 `save_memory(text)`에 대해 `store.add_memory(text, "chat")` 실행 → 어시스턴트 tool_call 에코 + `role:"tool"` 결과("saved") 메시지를 `messages`에 append → 다음 회차로.
   - `text`가 있으면 반환(종료).
4. 3회 초과 시 마지막 텍스트(없으면 안내 문구) 반환.

**Graceful degrade**:

- 엔진이 툴콜 없이 텍스트만 반환(툴 미지원 모델) → 저장 no-op, 채팅은 정상.
- `tools` 파라미터로 엔진이 4xx/5xx 에러 → **tools 없이 1회 재시도**하여 채팅이 절대 깨지지 않게 한다.
- 신뢰 경로는 항상 열려 있는 수동 관리 UI.

**확인 UX**: 툴 결과를 받은 모델이 최종 텍스트에서 자연스럽게 "기억했어요!"라고 답한다 — 별도 확인 장치 불필요.

### 4. 관리 UI

**커맨드**(`commands.rs`) + **API**(`api.ts`, `invoke<T>('...')` 패턴):

| 커맨드 | 시그니처 |
|--------|----------|
| `memory_list` | `() -> Vec<Memory>` |
| `memory_add` | `(text) -> Memory` |
| `memory_update` | `(id, text) -> Memory` |
| `memory_delete` | `(id) -> ()` |

**UI**: `LifeSettingsTab.svelte`에 "주인 메모리" 카드 — 메모리 목록(생성일 표시) + 인라인 편집 + 삭제 버튼 + 신규 추가 입력. 전용 탭은 초기 범위에서 과함.

### 5. 프라이버시 / 전송 경계 결정

CLAUDE.md 원칙: "트랜스크립트는 기본 로컬 처리. 외부 전송은 Engine 선택(사내 on-prem 기본)으로만." 메모리 텍스트가 설정된 엔진으로 가는 것은 "외부 전송은 Engine 경계로만" 규칙과 **일관**되나, 파생 수치가 아닌 **최초의 사용자 자유 텍스트 전송**이라는 점에서 전송 경계의 의미 있는 확장이다.

- 되돌리기 어려운 결정(스키마 신설 + 경계 완화)이므로 **ADR 후보**로 플래그한다. 본 스펙에 결정 근거를 남기고, ADR 작성 여부는 후속 판단(이 스펙이 채택되면 `docs/adr/`에 기록 권장).
- 사용자는 명시적으로만 메모리를 저장하며(자동 추출 없음), 언제든 UI에서 조회·삭제할 수 있어 통제권이 사용자에게 있다.

## 테스트 계획

**Rust** (`cargo test`):

- Store CRUD: add/list(정렬)/update(updated_at)/delete/count, 빈 텍스트 거부.
- `memory_block`: 포맷, 빈 목록 시 빈 문자열, 캡(개수·문자수) 초과 시 최신 유지 + 드롭 로그.
- `chat_send` 루프: MockEngine이 캔드 `save_memory` 툴콜 → `add_memory` 호출 확인 → 이어지는 텍스트 반환.
- Graceful degrade: 엔진 tools 에러 시 tools 없이 재시도해 텍스트 반환.
- 기존 `classify_intent`·프롬프트 테스트 불변(회귀 없음), 프롬프트에 메모리 블록 주입 확인.

**Frontend** (`npm test`, Vitest):

- 메모리 관리 컴포넌트: 목록 렌더, 추가/편집/삭제 시 대응 `invoke` 호출(api 목).

## 범위 밖 (YAGNI) / 후속

- 분류/태그, 만료(TTL), 자동 추출, 채팅으로 잊기(`forget_memory` 툴), 관련도 랭킹 — **모두 미구현**. 필요 시 후속 스펙에서 확장.
- 삭제·편집은 UI 전용(채팅으로 지우기 없음).
