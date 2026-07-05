# Agent Mentor — 프론트엔드 2단계(마스코트) 설계

> **상태**: 상세화 완료 (2026-07-03). 비전 스펙 `2026-07-03-frontend-vision-design.md` §1·§5의 합의를 구현 수준으로 구체화한 것 — 새 브레인스토밍 없음.
> **선행**: 1단계 셸+데이터 (PR #8, `feat/frontend-shell`). 이 스펙의 브랜치 `feat/frontend-mascot`는 그 위에 스택.
> **범위**: mascot 창 + 픽셀 로봇 절차 생성·애니메이션 + 말풍선 + 트리거 4종 + 트레이 "마스코트 표시" 토글.
> **비범위**: 미니룸(3단계), 배회형 이동(비전에서 제외), LLM 잡담(엔진 호출 없음 — 잡담은 정적 템플릿).

---

## 1. mascot 창

| 항목 | 값 |
|---|---|
| label | `mascot`, 엔트리 `mascot.html` (Vite 멀티페이지 입력 추가) |
| 크기 | 기본 **160×160**, 말풍선 표시 중 **320×230** (프론트가 setSize, 소멸 시 복원) |
| 속성 | `transparent: true`, `decorations: false`, `alwaysOnTop: true`, `skipTaskbar: true`, `resizable: false`, `visible: false` (설정 보고 setup에서 표시) |
| 이동 | 캐릭터 영역 `data-tauri-drag-region`으로 드래그만. 이동 종료를 1초 디바운스해 `set_setting("mascot_pos", "x,y")` 저장 |
| 위치 복원 | Rust setup: `get_setting("mascot_pos")` 있으면 `set_position`, 없으면 화면 우하단(작업표시줄 위) 기본 |
| 표시 토글 | 트레이 메뉴에 `마스코트 표시` CheckMenuItem 추가 — `mascot_visible` 설정(기본 "true") 연동, show/hide |
| capabilities | mascot 창: `core:default` + `core:window:allow-set-size` + `core:window:allow-set-position` + `core:window:allow-start-dragging` |

말풍선 확장 시 좌상단 기준 setSize만 하면 캐릭터가 화면상 이동하므로, **확장 시 위치를 (x−80, y−70) 보정하고 복원 시 되돌린다** (캐릭터는 창 우하단 고정 배치).

## 2. 로봇 렌더 (파츠 라이브러리)

- **데이터** (`src/lib/robot/parts.ts`): 슬롯은 RobotSpec과 동일(antenna/head/eyes/body/arms + palette). 각 슬롯 변형 6종, 팔레트 8종 — **Rust 상수(ANTENNA_VARIANTS=6 … PALETTE_VARIANTS=8)와 개수가 일치해야 하며 vitest가 이를 단언**한다. 파츠는 **32×32 그리드**(`GRID` 상수) 위 `[x, y, colorIndex][]` 배열. 팔레트는 **8색**(`body`, `bodyShade`, `accent`, `accentShade`, `eye`, `outline`, `highlight`, `cheek`) × 8종 레트로 톤 — 음영·하이라이트·볼터치로 싸이월드 미니미 수준의 입체감을 낸다. 비례는 2등신(머리 크게, 몸통 짧게)이 기본.
  - *이력: E2E 피드백(픽셀이 크고 밋밋함)으로 16×16·4색에서 상향 (2026-07-05).*
- **조립** (`src/lib/robot/render.ts`): `buildRobotPixels(spec: RobotSpec, frame: Frame): Pixel[]` 순수 함수 — canvas 무관, vitest로 결정성·경계(0≤x,y<GRID) 테스트. `drawRobot(ctx, spec, frame)`이 canvas에 32×32 → CSS `image-rendering: pixelated` **4배**(128px, 화면 크기 불변) 표시.
- **시드**: 기존 커맨드 `get_mascot_seed` → RobotSpec 재사용 (백엔드 변경 없음).

## 3. 애니메이션 상태 머신 (`src/lib/robot/anim.ts`)

| 상태 | 진입 조건 | 표현 (파츠 오프셋/스왑 — 모든 조합 공통) |
|---|---|---|
| `idle` | 기본 | 1.6초 주기 바운스(±1px), 3~6초 랜덤 깜빡임(eyes 스왑 2프레임) |
| `talk` | 말풍선 표시 중 | 눈 LED 점멸 + 바운스 짧게 |
| `happy` | diary:ready·occasion 말풍선 표시 중 | 점프 반복 + 눈 ^^ 스왑 (말풍선 수명 6초에 종속) |
| `alert` | finding 말풍선 표시 중 | 안테나 점멸(accent색 토글), 말풍선 종료까지 (severity 무관 — 상태 머신은 말풍선 종류만 본다) |
| `sleep` | 로컬 01:00–07:00 & 말풍선 없음 | 눈 감김 스왑 + zZ 픽셀, 잡담 금지 |

구현: `requestAnimationFrame` 단일 루프, 상태는 `$state`, 프레임 계산은 순수 함수 `frameAt(state, tMs)` (vitest 테스트 대상).

## 4. 말풍선 (`src/lib/robot/bubble.ts` + Mascot.svelte)

- **큐**: FIFO, 최대 5건(초과 시 가장 오래된 것 드롭), 표시 6초 + 간격 0.5초.
- **모양**: 픽셀 보더(3px 계단 보더), 최대 2줄(ellipsis), 캐릭터 위에 표시.
- **클릭**: 말풍선 클릭 → `invoke("open_chat_tab", { tab })` — finding→`coach`, diary→`diary`, occasion·잡담→`home`. 백엔드가 chat 창 show/focus 후 `emit("chat:goto-tab", tab)`, chat의 App.svelte가 수신해 탭 전환.
- **텍스트 템플릿** (`bubbleText(payload)` 순수 함수, vitest):
  - finding(복수면 최대 절약 1건 + "외 N건"): `"주인, R1 안 쓰는 MCP가 있어요 (~30,000 tok 아까움!)"` — rule_id별 한 줄 문구 맵(R1/R2/R5/R7/R9), 미지 rule은 generic.
  - diary: `"어제 일기 다 썼어요! 보러 올래요?"`
  - occasion: `"오늘 {label}이래요! 🎉"` (첫 라벨만)
  - 잡담: 풀에서 랜덤 (아래 §5)

## 5. 트리거 4종 배선

| 트리거 | 소스 | 비고 |
|---|---|---|
| 새·악화 finding | 기존 `coach:finding` 이벤트 (payload FindingRow[]) | 백엔드 변경 없음 |
| 다이어리 완성 | 기존 `diary:ready` (payload date) | 백엔드 변경 없음 |
| occasions | **신규** `occasion:today` (payload `Vec<String>` labels) | 파이프라인 스캔 완료 시: `compute_occasions(오늘)` 비어있지 않고 `occasion_notified_date` 설정 ≠ 오늘이면 emit 후 설정 갱신. 판단은 순수 함수 `should_notify_occasion(today, last) -> bool`로 분리(테스트) |
| 유휴 잡담 | mascot 프론트 타이머 | `chatter_level` 설정: `"off"\|"low"\|"normal"`, **기본 "low"**. low=60~120분, normal=20~40분 랜덤 간격. sleep 시간대·말풍선 진행 중엔 스킵. 문구 풀 ~10종, 일부는 `get_summary` 수치 삽입("오늘 벌써 {n}세션이나 돌렸어요") |
| **실시간 수집 요약** | `scan:done` 이벤트 | `realtime_advice` 설정(`"off"\|"on"`, **기본 off**) — 켜면 매 수집 완료마다 즉시 요약 말풍선("방금 세션 반영 — 오늘 N세션, 절약 가능 ~M tok", 클릭→홈 탭). 트레이 메뉴 `실시간 조언` CheckMenuItem으로 토글(설정 변경 시 mascot에 `settings:changed` emit → 리로드 없이 반영). 새 finding 말풍선과 중복 시 큐가 순차 처리. *이력: E2E 피드백 — 실시간으로 조언 듣고 싶은 사용자 옵션 (2026-07-05).* |

## 6. 백엔드 변경 (src-tauri + core)

- `tauri.conf.json`: mascot 창 추가. capabilities에 mascot 창·권한 추가.
- `lib.rs` setup: mascot_visible(기본 true)이면 mascot 창 show + 위치 복원.
- `tray.rs`: `마스코트 표시` CheckMenuItem (open과 scan 사이) — 토글 시 show/hide + `set_setting("mascot_visible", ...)`.
- `tray.rs`: `실시간 조언` CheckMenuItem (마스코트 표시 다음) — 토글 시 `set_setting("realtime_advice", ...)` + `emit("settings:changed", ())` (mascot이 설정 재로드). `set_setting` allowlist에 `realtime_advice` 추가.
- `pipeline.rs`: 스캔 성공 블록에서 occasion 판정·emit (다이어리와 동일하게 실패 무해화 — eprintln만).
- `commands.rs`: `open_chat_tab(tab: String)` 신규(허용 탭 home/diary/coach/chat 검증). `set_setting` allowlist에 `"mascot_pos"` 추가.
- core 변경 없음. occasion 계산은 기존 `diary::occasions::compute_occasions(date, anchor, locale, include_dev_days)` 재사용 — `anchor`는 `store.earliest_session_ts()`의 날짜(함께한 지 N일 마일스톤 기준), `locale`은 `resolve_locale(&DiaryConfig::default())`, `include_dev_days=true`. emit payload는 `Vec<String>`(각 `Occasion.label`).

## 7. 프론트 구조

```
src/
├─ mascot.html / mascot.ts          # 신규 엔트리 (vite input 추가)
├─ Mascot.svelte                    # 캔버스 + 말풍선 + 드래그 + 상태 머신 구동
└─ lib/robot/
   ├─ parts.ts      # 파츠·팔레트 상수 (데이터만)
   ├─ render.ts     # buildRobotPixels(순수) + drawRobot(canvas)
   ├─ anim.ts       # frameAt(state, t) 순수 상태 머신
   └─ bubble.ts     # 큐 로직 + bubbleText 템플릿 (순수)
```

chat 쪽: `App.svelte`에 `chat:goto-tab` 리스너 추가(탭 전환)만.

## 8. 테스트

- **vitest 도입** (신규 devDep, `npm test`): parts 무결성(모든 파츠 16×16 경계·변형 수 = Rust 상수 6/8과 일치), buildRobotPixels 결정성, frameAt 상태 전이(sleep 시간대·talk 우선), bubble 큐(드롭·순서), bubbleText 템플릿.
- **Rust**: `should_notify_occasion` 단위 테스트, `open_chat_tab` 탭 검증 테스트. 기존 99개 무회귀.
- 렌더 화질·애니메이션 감성은 수동 확인 (E2E 체크리스트).

## 9. 수동 E2E 체크리스트

- [ ] 마스코트가 우하단에 표시, 사용자별 로봇 모양(시드) 확인, idle 바운스·깜빡임
- [ ] 드래그 이동 → 앱 재시작 후 위치 복원
- [ ] 트레이 "마스코트 표시" 토글 동작·재시작 후 유지
- [ ] 세션 활동 → 새 finding 말풍선 (warn이면 alert 안테나 점멸) → 클릭 시 chat 코칭 탭
- [ ] `AGENT_MENTOR_ENGINE_URL` 설정 후 자정 넘김(또는 DB 조작) → diary:ready 말풍선 → 다이어리 탭
- [ ] 잡담 주기(chatter_level normal로 20~40분) 및 sleep 시간대 침묵
- [ ] 말풍선 표시 중 창 확장·복원이 캐릭터 위치를 유지하는가
