---
status: done
archived: 2026-07-31
---

# 묶음 ⑥ — 타입 체크 안전망 + 방문 소품 (Q3·Q2·Q1)

- **날짜**: 2026-07-31
- **컴포넌트**: a-mate (프론트엔드 전용 — Rust·a-hub 미접촉)
- **관계**: [Life 소셜·일기 통합 후속 로드맵](../plans/2026-07-26-life-social-diary-followups-roadmap.md) **8차 배치 Q1–Q3**.
  묶음 ④(PR #132) 구현 중 식별했으나 의도적으로 범위 밖에 둔 잔여 후속 3건.
- **목적**: 이 레포에 존재하지 않던 **타입 체크 단계를 도입**하고, 같은 프론트 영역의
  잔여 소품 2건을 한 브랜치에서 함께 정리한다.

## 1. 배경

묶음 ④ 플랜은 "`Notice['kind']`에 값을 추가하면 `Record` 맵이 컴파일로 강제된다"를 안전망으로 전제했다.
그 전제가 **실제로는 작동하지 않는다**는 것이 8차 배치 Q3 조사에서 실험으로 확인됐다 —
`NoticeLog.svelte`의 맵에서 `guestbook` 키를 지운 채 `npm run build`는 exit 0,
`npm test`는 전부 통과했다. 런타임 결과는 아이콘이 빈칸일 뿐 어디에서도 잡히지 않는다.

원인은 단순하다. `package.json` 스크립트가 `dev`/`build`/`tauri`/`test`뿐이고
`build` = `vite build`(transpile-only)다. `svelte-check`는 devDependency에 없고,
`vite.config.ts`에도 체커 플러그인이 없다. `typescript`는 설치돼 있으나 **아무것도 `tsc`를 호출하지 않는다.**

## 2. 범위

| 항목 | 내용 | 성격 |
|---|---|---|
| **Q3** | `svelte-check` 도입 + 드러난 실오류 해소 | 인프라 |
| **Q2** | 방문 말풍선의 마스코트 표정 | 소품 (3줄) |
| **Q1** | 방명록 읽음 워터마크의 최초 시드를 서버 시각으로 | 소품 |

**커밋 순서 = Q3 → Q2 → Q1** (항목당 커밋 분리).

Q3를 먼저 두는 이유는 순서 취향이 아니다. Q1은 `UnseenState.guestbookLastSeen`의
**타입 시그니처를 `string` → `string | null`로 바꾸는 변경**이라 소비처 누락 위험이 실재하고,
그것이 정확히 `svelte-check`가 잡는 종류의 결함이다. 안전망을 먼저 깔면 도입 효과를
같은 PR 안에서 실증하게 된다.

## 3. Q3 — svelte-check 도입

### 3.1 사전 실측 (2026-07-31, `npx svelte-check`, 로컬 `node_modules` 기준)

로드맵은 "한 번도 타입 체크된 적이 없어 누적 오류 규모 파악이 먼저"라며
필요하면 `--threshold error`로 **단계 도입**하라고 적었다. 실측 결과 단계 도입은 불필요하다.

**총 78 errors / 13 warnings / 20 files** — 내역이 결론을 바꾼다.

| 출처 | Error 건수 | 성격 |
|---|---|---|
| `node_modules/@types/node` | 40 | tsconfig에 `skipLibCheck` 없음(기본 `false`) |
| `node_modules` svelte·esrap·vitest·rollup `.d.ts` | 31 | 동일. 서로 다른 패키지의 전역 타입 충돌(`Node`·`Visitors` 중복 등) |
| **`src/` — 우리 코드** | **7** | §3.3 |

즉 78건 중 **71건이 라이브러리 `.d.ts`**이고, 우리가 고칠 수 없는 종류다.
`skipLibCheck: true` 한 줄로 소거된다.

### 3.2 설정 변경

```jsonc
// a-mate/tsconfig.json — compilerOptions에 추가
"skipLibCheck": true,
"types": ["vite/client", "node"]
```

```jsonc
// a-mate/package.json
"devDependencies": {
  "svelte-check": "^4",
  "@types/node": "^24"   // 런타임 메이저에 맞춤 — 이 환경은 Node v24.11.0
},
"scripts": {
  "check": "svelte-check --threshold error",
  "test":  "svelte-check --threshold error && vitest run"
}
```

**`skipLibCheck: true`는 회피가 아니라 표준 관행이다.** 라이브러리 `.d.ts`는 소비자가 고칠 수 없고,
패키지 간 전역 타입 충돌은 우리 코드의 결함을 가리키지 않는다. 우리 코드에서 그 타입을
*사용하는* 지점은 여전히 검사된다.

**게이트 방식 = `npm test` 편입.** 이 레포엔 `.github/workflows`가 없어 CI 게이트가 불가능하고,
검증은 로컬 관행(`cargo test` / `npm test` / `npm run build`) 3종이 전부다.
별도 `check` 스크립트만 두면 CI 없는 레포에서는 아무도 돌리지 않아 사문화된다.
`npm test`에 묶으면 기존 관행에 자동 편입된다. 단독 실행용 `check` 스크립트도 함께 남긴다.

**검사 강도 = `--threshold error`.** 경고 13건은 출력에 남기되 실패로 치지 않는다.
경고의 절반은 `LifeView.svelte`의 `<polygon>` a11y 6건인데, 아이소메트릭 방 격자 클릭에
키보드 대응을 붙이는 것은 별도 UI 설계 과제다. 이 소품 PR에 끌어들이면 범위가 터진다.

### 3.3 드러난 실오류 — 원인 4종 / 진단 7건

| 원인 | 위치 (진단 수) | 진단 | 수정 |
|---|---|---|---|
| A | `lib/ui/settings/ConnectionGroup.svelte:18` (1) | `Property 'api_key' does not exist on type 'HubSettings'` | `api.ts:137` 인터페이스에 `api_key: string;` 추가 |
| B | `lib/interior/catalog.ts:48` · `catalog.test.ts:169` (2) | `ground: number[]` → `[number, number]` 캐스팅 불가 (TS2352) | `AssetMetric.ground`를 `number[]`로 완화 (테스트의 인라인 타입도 동일 처리) |
| C | `lib/interior/catalog.test.ts:152` (1) | `'sourceAxes' is possibly 'undefined'` | `sourceAxes?.[source]`로 옵셔널 체이닝 |
| D | `lib/no-hardcoded-colors.test.ts:2,3,32` (3) | `node:fs`·`node:path`·`process` 미해결 | §3.2의 `@types/node` + `types` 설정으로 해소 (**코드 무변경**) |

**A는 진짜 드리프트다.** 백엔드 `src-tauri/src/commands.rs:817`의 `hub_settings_get`이
`api_key: get("hub_api_key")`를 **실제로 내려주고 있고**, 프론트 `loadHub()`가 그 값을 읽어
연결 설정의 password 입력창을 채운다. 런타임은 정상 동작하며, **TS 선언만 실제와 어긋나 있었다.**
타입 체크가 없어 3개월간 드러나지 않은 종류의 결함이고, 이 항목 하나가 Q3 도입의 정당성을 보여준다.
수정은 선언을 실제에 맞추는 것이지 동작 변경이 아니다.

**B의 처리 방침 — `as unknown as` 이중 캐스팅을 쓰지 않는다.**
`assetMetricsJson`은 JSON import라 `ground`가 `number[]`로 추론되고,
`[number, number]` 튜플로의 직접 캐스팅은 TS가 거부한다. 두 선택지가 있다.

| 안 | 결과 |
|---|---|
| `as unknown as Record<string, AssetMetric>` | 타입은 튜플로 정확해 보이지만, **JSON에 길이 3 배열이 와도 통과**한다 — 검증 없는 거짓말이 남는다 |
| **`ground: number[]`로 완화** (채택) | JSON이 실제로 주는 타입 그대로. 거짓말이 없다 |

전수 확인 결과 `ground` 사용처는 `catalog.ts:310,311,319,321`과 테스트 2곳뿐이고
**전부 `ground[0]`/`ground[1]` 인덱스 접근**이다. 구조분해가 없어 튜플일 필요가 없다.
`noUncheckedIndexedAccess`는 tsconfig에 없으므로(`strict`만 존재) 인덱싱 결과는 `number`로 유지된다.
따라서 타입 완화가 동작·표현력 어느 쪽도 잃지 않으면서 거짓말만 제거한다.

**C는 테스트만의 누락이다.** `FLOOR_SOURCE_AXES_BY_ASSET`은 바깥이 `Partial<Record<string, …>>`이라
`Object.entries()`가 뽑는 값이 `undefined`일 수 있다. 프로덕션 코드(`catalog.ts:283`)는
`FLOOR_SOURCE_AXES_BY_ASSET[assetId]?.[view.source]`로 **이미 옵셔널 체이닝을 쓰고 있고**,
테스트만 그 처리를 빠뜨렸다. 프로덕션과 같은 형태로 맞춘다.

## 4. Q2 — 방문 말풍선의 마스코트 표정

**증상**: `robot/anim.ts:13`의 `resolveState`에 `visit` 분기가 없어 `default`로 떨어진다.
`default`는 시각으로만 판정하므로 **01\~07시엔 `sleep`** — "○○님이 놀러왔어요!"를 자는 표정으로 말한다.
`BubbleKind`(`robot/bubble.ts:1`)에는 `visit`이 정상적으로 존재하고, `bubble.ts:50`이 그 말풍선을 만든다.
분기 누락만의 문제다.

**수정**: `diary`·`occasion`과 동일 취급.

```ts
case 'diary':
case 'occasion':
case 'visit': return 'happy';   // ← 추가
```

`happy`는 `frameAt`에서 웃는 얼굴(`expression: 'happy'`) + 200ms마다 위로 톡톡 튀는 애니메이션이라
"누가 놀러왔다"는 기쁜 소식과 결이 맞는다. `anim.test.ts`에 케이스 1건을 추가한다.

## 5. Q1 — 방명록 읽음 워터마크의 최초 시드

### 5.1 증상

`App.svelte:152`의 `loadUnseen(new Date().toISOString())` → `parseUnseen`이
저장분이 **없거나 손상**됐을 때 `guestbookLastSeen`을 **클라이언트 시계**로 초기화하고
곧바로 고정한다(`unseen.ts:23`). 이 값은 서버가 발급한 `created_at`과 사전순 비교되므로
시계 오차 Δ만큼 어긋난다.

| 시계 상태 | 결과 |
|---|---|
| 빠름 | 서버 시각 기준 Δ 동안 들어온 타인 글이 뱃지에 안 잡힘 |
| 느림 | 부트스트랩이 전체 방명록을 조회하므로 설치 직전 Δ 구간의 과거 글이 뱃지로 잡힘 |

**영향은 뱃지 카운트에 국한된다.** 홈 최근 알림은 백엔드 커서(`inbound_guestbook_cursor`,
서버 시각만 사용)로 판정하므로 소식 자체는 유실되지 않는다.

### 5.2 왜 지금 고치나

자기 교정 조건이 약하다. 워터마크가 서버 시각으로 바뀌려면 **뱃지 클리어가 1회 일어나야** 하고,
그러려면 뱃지가 뜨고 사용자가 **자기 방** 방명록 탭을 열어야 한다. 탭을 한 번도 안 열면
시드값이 무기한 유지된다. (반복 사용되는 클리어 경로 자체는 PR #132 `cb3a8dc`에서 이미
서버 시각으로 수정됐다 — 남은 건 최초 시드뿐이다.)

### 5.3 설계 — nullable 시드

올바른 방향은 **첫 부트스트랩에서 서버 데이터로 시드**하는 것이다
(기존 글 전체의 `max(created_at)` = "이미 있는 건 다 본 것").
그러려면 "아직 시드 안 됨" 상태를 표현할 수 있어야 하는데,
현재 `parseUnseen`은 없음·손상을 같은 반환 형태로 뭉개 신규/저장분을 구별할 수 없다.

| # | 변경 |
|---|---|
| 1 | `UnseenState.guestbookLastSeen: string \| null` — `null` = "아직 시드 안 됨" |
| 2 | `parseUnseen`이 없음·손상 시 `null` 반환. `nowIso` 인자가 불필요해지므로 제거 (`loadUnseen`도 동일) |
| 3 | `newGuestbookIds(rows, meId, null)` → **0건 반환**, 호출부가 그 조회의 `max(created_at)`으로 시드 |
| 4 | 방명록이 비어 있으면 시드할 값이 없으므로 `null` 유지 → 다음 첫 글이 정상적으로 신규로 잡힌다 (의도된 동작) |
| 5 | 조회 실패(오프라인·구서버 404)도 `null` 유지 → 다음 성공 조회 때 시드 |

### 5.4 버린 우회 (기록)

"시드가 서버 최신 글보다 미래면 끌어내린다"는 **무조건 클램프는 정상 동작을 깨뜨린다.**
방명록을 다 읽은 사용자는 정의상 `lastSeen > max(created_at)`이므로,
클램프하면 이미 읽은 글이 되살아난다. 신규 여부를 알아야만 안전하다는 점이
nullable 시드가 유일한 길임을 확인해 준다.

서버에 시각을 직접 묻는 방법도 기각한다 — `/life/me`가 타임스탬프를 주지 않아
**life 서버 계약 변경**([life-visit.md §4](../../../../design/life-visit.md)가 정본)이 필요하고, 이 소품에 비해 과대하다.

## 6. 검증

| 대상 | 명령 | 베이스라인 (2026-07-31 워크트리 실측) |
|---|---|---|
| 프론트 | `npm test` | 210 passed / 26 files |
| Rust | `cargo test` | exit 0 (미접촉 — baseline 확인용) |
| 빌드 | `npm run build` | — |

Q1·Q2는 순수 함수 계층(`unseen.ts`·`anim.ts`)이라 Vitest로 전부 덮인다
(`unseen.test.ts`·`anim.test.ts` 확장).

**실환경 스모크**: Q2는 01\~07시에 방문 알림이 떠야 육안 확인이 가능해 재현이 까다롭다.
표정 매핑은 단위 테스트로 덮고, 육안 확인은 PR 체크리스트의 **선택 항목**으로 둔다.

## 7. 범위 밖 (YAGNI)

| 항목 | 이유 |
|---|---|
| a11y 경고 13건 해소 | 방 격자 `<polygon>` 키보드 대응은 별도 UI 설계 과제 (§3.2) |
| `api_key`를 프론트로 내려주는 설계 자체의 재검토 | 로컬 앱이고 키는 사용자 본인 것. 이번 수정은 **선언을 실제에 맞추는 것**이지 동작 변경이 아니다 |
| CI 워크플로 신설 | 이 PR은 타입 체크 도입이 목적. CI 자체는 별개 결정 |
| `noUncheckedIndexedAccess` 등 strict 옵션 강화 | 도입 PR에서 규칙까지 조이면 오류 규모가 다시 미지수가 된다 |
