# G7 — 방명록 작성자 아이콘: 봇/사람 구분 + 관찰자 불변

- 날짜: 2026-08-02
- 대상: a-mate 방명록 UI(`GuestbookTab`), Life 클라이언트 커맨드, 스캔 파이프라인
- 관련: [ADR 0022](../../../adr/0022-guestbook-bot-reply-label.md)(§결과에서 "타 방문자 봇 라벨/아바타"를
  후속 과제로 유보 — 본 스펙이 그 후속), [ADR 0020](../../../adr/0020-owner-fullname-to-hub.md)(사람 작성 풀네임 서명),
  [ADR 0021](../../../adr/0021-guestbook-replies-one-depth.md)(1단계 답글·주인 전용)
- a-hub 변경: **없음** (서버 스키마·API·계약 모두 현행 유지)

## 1. 배경

방명록 항목에 아바타가 붙는 조건은 현재 하나뿐이다.

```ts
// a-mate/src/lib/guestbook.ts:22
export function showOwnerAvatar(entry, meId, isOwner) {
  return isOwner && entry.author_agent_id === meId;
}
```

여기서 세 가지 문제가 나온다.

| # | 문제 | 근거 |
|---|------|------|
| P1 | **봇/사람이 구분되지 않는다** | 서버에 `author_kind`가 이미 저장·에코되는데 UI가 읽지 않는다 |
| P2 | **보는 사람에 따라 화면이 다르다** | 판정에 `meId`·`isOwner`가 들어가 관찰자마다 결과가 갈린다 |
| P3 | **같은 앱 안에서 방과 방명록이 어긋난다** | 방(LifeView)은 타인 캐릭터를 이미 그리는데 방명록만 못 그린다 |

P2가 특히 어긋나 있다. 방명록은 `GET /life/{id}/guestbook`이 **인증 헤더조차 받지 않는** 완전 공개
데이터인데(`api.py:254`), 렌더만 로컬 상태에 좌우된다. 답글·삭제 버튼이 관찰자에 따라 다른 것은
권한 UI라 정상이지만, 아이콘은 권한과 무관하다.

P1은 데이터가 아니라 소비의 문제다. `author_kind`는 이미 모든 작성 경로에서 정확히 채워진다.

| 경로 | 값 | 위치 |
|------|-----|------|
| 사람이 UI에서 작성 | `Some("human")` | `commands.rs:1137` |
| 봇 자동 답글 | `Some("bot")` | `pipeline.rs:1182` |
| 봇 방문 인사 | `Some("bot")` | `visit.rs:418` |
| 구버전 클라이언트 | `None` | 서버 규약상 human 간주 (`test_life.py:456`) |

## 2. 목표 / 비목표

**목표**

1. 사람이 남긴 글과 봇이 남긴 글이 아이콘만으로 구분된다.
2. 같은 방명록 항목은 **누가 어디서 보든 같은 아이콘**으로 보인다. 남의 방에서도 동일하다.

**비목표**

- 사람의 실제 프로필 사진 — 그런 데이터가 시스템에 없고, 도입은 별개의 프라이버시 결정이다.
- 방명록 외 화면(방·소식·다이어리)의 아이콘 규칙 변경.
- `author_name` 스푸핑 검증 — ADR 0020 §결과가 후속 과제로 남긴 그대로 둔다.

## 3. 결정

### 3.1 아이콘 결정은 엔트리만의 함수

```ts
export type AuthorIcon =
  | { kind: 'bot'; agentId: string }
  | { kind: 'monogram'; initial: string; hue: number }

export function authorIcon(entry: GuestbookEntry): AuthorIcon
```

**관찰자를 나타내는 인자가 없다.** `meId`·`isOwner`·`myLifeId` 중 무엇도 받지 않는다.
목표 2가 관례가 아니라 시그니처로 강제된다 — 관찰자 정보를 넣으려면 타입을 바꿔야 한다.

분기는 `author_kind` 하나로 결정한다.

```
author_kind === 'bot'  → { kind: 'bot' }
그 외('human' | null)  → { kind: 'monogram' }
```

### 3.2 봇 — 서버에 게시된 마스코트 얼굴

소스는 `GET /life/agents/{agent_id}/mascot-image`다. 인증만 통과하면 친구 여부와 무관하게
누구나 받을 수 있어(`life.py:486`) 목표 2를 만족한다.

신규 Tauri 커맨드 `life_mascot_face(agent_id)`를 추가한다. 기존 `life_mascot_image`
(`commands.rs:1161`)에 `crop_face`를 얹은 형태다.

- `crop_face`(`sprite.rs:345`)는 파일 경로가 아니라 **바이트를 받는 순수 함수**라 서버에서 받은
  타인 PNG에 그대로 적용된다. 크롭 기준(128×128, 시각 검증된 CSS 환산)도 내 얼굴과 동일하다.
- 기존 `life_mascot_image`는 **그대로 둔다.** 방(LifeView)이 전신 렌더에 쓰고 있어
  크롭을 섞으면 방 안 로봇이 얼굴만 남는다.

프론트는 `agent_id → base64` 맵으로 중복 요청을 접는다. 한 화면의 고유 작성자는 보통 2~5명이다.

**내 봇 글도 예외 없이 이 경로를 쓴다.** 로컬 `getFaceIcon()`을 쓰면 서버 업로드 전에
"나만 얼굴이 보이는" 상태가 되어 목표 2가 다시 깨진다.

### 3.3 봇 폴백 — 🤖 이모지 (현행 유지)

서버에 이미지가 없거나 조회에 실패하면 지금의 `ava-fb` 이모지 폴백을 그대로 쓴다.
얼굴 없는 봇끼리는 구분되지 않지만, 3.5의 업로드 보장이 있으면 드문 예외
(이미지 모델 미설정·구버전 클라이언트)로 좁혀진다. 이 예외를 위해 별도 시각 체계를
만들지 않는다.

### 3.4 사람 — 이니셜 모노그램

`author_name`에서 파생한다. 이름은 이미 방명록에 전부 공개되므로 새 노출이 아니다.

| 함수 | 규칙 |
|------|------|
| `authorInitial(name)` | 첫 글자 1개. 빈 이름·공백뿐이면 `?` |
| `authorHue(name)` | 이름 해시 → 0~359. 결정론적 |

- 배경은 `hsl(hue, 고정 채도, 고정 명도)`, 텍스트는 흰색 고정 — hue가 어떤 값이어도 대비가 보장된다.
- 첫 글자는 **코드 유닛이 아니라 코드포인트 기준**으로 자른다. 서로게이트 페어(이모지)를 반으로
  쪼개면 깨진 문자가 나온다.
- 사람이 `owner_full_name`을 설정하지 않았으면(ADR 0020의 옵트인) 서버가 봇 이름으로 fallback한
  값이 들어온다. 그 이름의 첫 글자를 쓴다 — 데이터 한계를 UI에서 보정하지 않는다.

테두리 형태는 봇과 같은 원형을 유지한다. 글자와 그림의 차이가 이미 충분히 크므로 형태까지
다르게 할 이유가 없고, 목록의 아이콘 열이 들쭉날쭉해진다.

### 3.5 업로드 보장 — 목표 2의 전제

현재 `lifeSyncMascotImage()`는 `LifeView` 마운트 시에만 호출된다(`LifeView.svelte:88`).
봇은 백그라운드로 자율 방문·자동 답글까지 하는데, 사용자가 방 탭을 한 번도 열지 않았다면
서버에 얼굴이 없어 남들에게 아이콘이 보이지 않는다.

스캔 파이프라인에 업로드를 1회 추가한다. 서버가 같은 sha면 쓰기를 생략하므로(`life.py:481`)
반복 비용은 조회 1회다. hub 미연결·스프라이트 없음이면 no-op이며, 실패는 warn+skip
(`maybe_*` 계열 규율과 동일).

이는 전송 데이터의 **종류**를 늘리지 않는다. 같은 이미지가 같은 엔드포인트로 갈 뿐이고,
시점이 "방 탭을 열 때"에서 "스캔할 때"로 앞당겨진다. 마스코트 이미지는 AI 생성 로봇 그림이라
ADR 0020이 다룬 실명 같은 신원 정보가 아니다.

### 3.6 제거

`showOwnerAvatar()`를 삭제한다. 이로써 `getFaceIcon()`의 유일한 소비자가 사라지므로
`get_face_icon` 커맨드와 `face_icon_inner`도 함께 제거한다. `crop_face`는 3.2의 신규 커맨드가
계속 쓰므로 남는다.

## 4. 데이터·계약

**변경 없음.** a-hub 스키마·엔드포인트·`contracts/` 모두 현행 그대로다. 필요한 필드가
이미 전부 저장·에코되고 있다.

| 필드 | 용도 | 출처 |
|------|------|------|
| `author_kind` | 봇/사람 분기 | `store.py:96` |
| `author_agent_id` | 봇 얼굴 조회 키 | `store.py:91` |
| `author_name` | 이니셜 글자·색상 시드 | `store.py:92` |

방명록 엔트리에 `mascot_image_sha256`을 **추가하지 않는다.** 방(LifeView)은 점유자 sha로 캐시를
무효화하지만, 방명록은 열려 있는 동안만 캐시를 들고 탭을 다시 열면 새로 받는다. 봇이 얼굴을
리롤한 경우 최대 그 화면이 닫힐 때까지 옛 얼굴이 남지만, 계약을 넓힐 만한 문제가 아니다.

## 5. 구현 범위

| 파일 | 변경 |
|------|------|
| `a-mate/src/lib/guestbook.ts` | `authorIcon`·`authorInitial`·`authorHue` 추가, `showOwnerAvatar` 삭제 |
| `a-mate/src/lib/ui/GuestbookTab.svelte` | 아이콘 렌더를 `authorIcon` 기반으로 교체(원글·답글 양쪽), agent_id별 얼굴 캐시, `getFaceIcon` 사용 제거 |
| `a-mate/src/lib/api.ts` | `lifeMascotFace` 추가, `getFaceIcon` 제거 |
| `a-mate/src-tauri/src/commands.rs` | `life_mascot_face` 추가, `get_face_icon`·`face_icon_inner` 제거 |
| `a-mate/src-tauri/src/lib.rs` | 커맨드 등록 교체 |
| `a-mate/src-tauri/src/pipeline.rs` | 스캔 시 마스코트 이미지 업로드 보장 |

`GuestbookTab`은 `meId`를 계속 받는다 — 삭제 버튼 권한 판정에 필요하다. 아이콘 경로에서만 빠진다.

## 6. 테스트

**vitest** (`guestbook.test.ts`)

- `authorIcon`: `'bot'` → bot, `'human'` → monogram, `null`(구데이터) → monogram
- `authorIcon`이 같은 엔트리에 대해 항상 같은 결과 — 관찰자 인자가 없음을 회귀로 고정
- `authorInitial`: 한글·영문·이모지(서로게이트 페어)·빈 문자열·공백만
- `authorHue`: 같은 이름 → 같은 값(결정론), 다른 이름 → 대체로 다른 값, 범위 0~359

**cargo**

- `life_mascot_face`의 크롭 적용 — `face_icon_inner` 선례대로 inner 함수를 분리해 테스트
- 기존 `crop_face` 테스트는 그대로 (동작 변경 없음)

**타입 체크**: `npm test`에 편입된 svelte-check가 `showOwnerAvatar`·`getFaceIcon` 잔존 참조를 잡는다.

## 7. 대안

- **사람도 픽셀 아바타(identicon)** — 이름 시드로 도트 패턴 생성. 화풍은 일관되지만 봇도 그림이라
  봇/사람 구분(목표 1)이 흐려진다. 기각.
- **사람도 절차 생성 캐릭터** — 방과 같은 `drawRobot` 폴백. 방명록 엔트리에 `mascot_seed`가 없어
  `author_name`을 시드로 써야 하는데, 그러면 방에서 보는 얼굴과 다른 얼굴이 나온다.
  목록에 캔버스 애니메이션을 여러 개 띄우는 비용도 붙는다. 기각.
- **고정 사람 실루엣** — 가장 단순하나 사람끼리 구분이 안 된다. 기각.
- **방명록 응답에 `author_mascot_seed`·`sha256` 추가** — 캐시 무효화와 시드 정확도는 좋아지지만,
  §4대로 지금 필드만으로 두 목표가 달성된다. 서버 계약을 넓힐 근거가 부족해 기각.
- **내 봇 글만 로컬 `getFaceIcon` 유지** — 서버 왕복을 아끼지만 업로드 전에 관찰자별 차이가
  생겨 목표 2에 정면으로 어긋난다. 기각.

## 8. 결과

- 방명록 아이콘이 관찰자와 무관해진다. ADR 0022가 유보한 "타 방문자 봇 아바타"가 해소된다.
- 방과 방명록이 같은 서버 이미지를 소스로 쓰게 되어 두 화면의 얼굴이 일치한다.
- 사람은 이름에서 파생한 모노그램을 갖는다. 이름을 바꾸면 아이콘도 바뀌지만, 방명록의
  `author_name`은 작성 시점 스냅샷이라(ADR 0020) 과거 글의 아이콘은 그대로 유지된다.
- 마스코트 이미지 업로드가 방 탭 열람과 무관해진다.
