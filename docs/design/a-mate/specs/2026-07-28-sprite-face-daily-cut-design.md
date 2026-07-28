# 스펙 — sprite 서브시스템: G6 얼굴 아이콘 + H2 매일 마스코트 컷

- 날짜: 2026-07-28
- 대상: a-mate `sprite.rs` 서브시스템 (묶음 ⑤ — [로드맵](../plans/2026-07-26-life-social-diary-followups-roadmap.md) G6·H2)
- 브랜치: `feat/sprite-face-daily-cut` (한 PR, G6/H2 커밋 분리)
- 관련: ADR 0019(전송 경계), ADR 0013(문서 수명주기), 신규 **ADR 0024**(이미지 엔진 전송 소재 수위 — 본 작업에서 작성. 0023은 half-depth-life-floor에 선점됨)

## 확정 결정 요약

| # | 결정 사항 | 확정 내용 |
|---|---|---|
| 1 | H2 소재 수위 (★프라이버시) | **일기 파생 추상 장면** — 텍스트 엔진이 일기 본문을 추상 장면 묘사로 변환. 고유명사·프로젝트명·코드 식별자 금지 지시. ADR 0024로 규범 기록 |
| 2 | H2 트리거·과금 가드 | 일기 훅(스캔 리컨실리에이션) + **일일 재시도 상한 3회** + **재실행 멱등**(마지막 일기 날짜 컷 존재 시 재생성 없음) |
| 3 | H2 보존 | **최신 1장만** (누적 없음 — 다시보기 스코프 외) |
| 4 | H2 캡션 | 일기 직후 **별도 텍스트 호출 1회**로 장면 묘사(영문)+캡션(한국어) 동시 생성 |
| 5 | H2 설정 | `daily_cut_enabled` **기본 off (옵트인)** |
| 6 | PR 구성 | 한 PR, G6/H2 커밋 분리 (묶음 ①·③ 선례) |
| 7 | G6 face 생성 시점 | **Lazy 재료화** — `get_face_icon`이 mtime 비교로 필요 시 크롭 (쓰기 훅 2곳 문제 해소) |
| 8 | H2 트리거 메커니즘 | **스캔 리컨실리에이션** (`maybe_generate_sprite` 선례 패턴) |
| 9 | H2 샷 축 선택 | **날짜 시드 결정** + LLM은 내용만 채움 (마스코트:정경 = 3:1 가중) |

---

## G6 — 마스코트 얼굴 아이콘 생성·저장

### 배경

G5 아바타(방명록 봇 답글)는 매 표시마다 전체 `sprite.png`를 로드해 CSS
(`background-size:180%`, `position 50% 14%`)로 얼굴을 크롭한다. 작은 아이콘에
큰 이미지 로드는 낭비 → 얼굴 영역을 잘라 작은 `face.png`로 캐시한다.

### 설계

| 위치 | 변경 |
|---|---|
| `crates/core/src/sprite.rs` | 신규 `crop_face(png_bytes: &[u8]) -> Result<Vec<u8>>` — png decode → RGBA 크롭 → **128×128 다운스케일(nearest-neighbor)** → encode. `make_background_transparent` 패턴 재사용, **새 의존성 없음** |
| `src-tauri/src/commands.rs` | 신규 `get_face_icon` 커맨드 — **Lazy 재료화**: `face.png` 없음 또는 `sprite.png`보다 mtime 오래됨 → 그 자리에서 `crop_face` 실행·저장 후 base64 반환. `sprite.png` 없으면 `None` |
| `src-tauri/src/lib.rs` | 커맨드 등록 (등록부는 병렬 작업과 소충돌 지점 — 머지 가능 수준) |
| `src/lib/api.ts` | `getFaceIcon(): Promise<string \| null>` |
| `src/lib/ui/GuestbookTab.svelte` | 아바타를 전체 sprite + CSS 줌에서 face 아이콘 `<img>`로 교체. `null`이면 기존 폴백 유지 |

### 크롭 환산 (CSS 검증값 → Rust)

시각적으로 확인된 CSS 크롭을 픽셀 연산으로 환산한다:

- 크롭 윈도우 한 변 = `min(W, H) / 1.8` (≈ 55.6%)
- 가로: 중앙 정렬 — `x = (W − w) / 2`
- 세로: `y = 0.14 × (H − h)` (CSS `background-position` 세로 14% 의미론)
- 결과를 128×128로 nearest-neighbor 다운스케일 (치비 픽셀아트 화풍 보존)

**실환경 육안 재검증 1회 필요** — PR 체크리스트 항목.

### 설계 노트

- 리롤 승격(`commands.rs` candidate→sprite.png) 등 **sprite 쓰기 경로에는 손대지
  않는다** — mtime 비교가 재생성·리롤 모두 자동 커버하고, 미래의 세 번째 쓰기
  지점도 자동 대응.
- 읽기 커맨드의 쓰기 부수효과는 캐시 재료화로 한정 (멱등, 실패 시 `None` 폴백).
- 방 점유자 라벨 등 다른 소비처로의 확장은 스코프 외 (GuestbookTab만 교체).

---

## H2 — 매일 바뀌는 마스코트 홈 컷

### 요구 (로드맵 H2)

새 일기 타이밍에 홈 좌측 상단 `RobotPortrait`를 매일 새 컷으로. 샷 축(풀샷/
바스트/클로즈업)·포즈·MBTI 성향 기반. 마스코트 없는 그림도 같은 화풍으로(일기
내용 반영). 그림 안 텍스트 금지, 같은 프레임 하단에 그림 설명 캡션 한 줄(오늘의
한마디와 별개). 싸이월드 미니홈피 감성(대문사진 + 감성 글귀) 참조.

### 데이터 흐름

```
스캔 말미 maybe_generate_daily_cut (maybe_generate_sprite 옆, pipeline.rs)
  게이트: daily_cut_enabled(기본 off) · 이미지 모델 설정 · 텍스트 엔진 설정
  ① 최신 일기 날짜 D = store.diary_dates() 최신 (없으면 no-op)
  ② 메타(app_data/daily_cut.json) 비교 — 리컨실리에이션 판단(순수 함수):
     date==D && 컷 파일 있음   → no-op (재실행 멱등)
     date==D && attempts >= 3  → no-op (그날 포기)
     그 외                     → 생성 시도 (date!=D면 attempts 리셋 0)
  ③ 샷 축 pick: 시드(D + mascot_seed)로 결정적
     [풀샷 | 바스트 | 클로즈업 | 정경(마스코트 없음)] 가중 1:1:1:1 → 마스코트:정경 = 3:1
     MBTI → 포즈·무드 힌트 (mbti_traits 결 재사용)
  ④ 텍스트 엔진 호출 1회: 일기 본문(D) + 샷 + MBTI 힌트
     → JSON { scene_en, caption_ko }
     프롬프트 지시: 고유명사·회사/프로젝트명·코드 식별자 금지 · 추상 장면 ·
     캡션은 싸이월드 감성 한국어 한 줄(마스코트 1인칭, daily_line과 별개)
  ⑤ 이미지 프롬프트 조립(로컬) = 치비 픽셀아트 화풍 프리픽스
     + (마스코트 샷이면 character_description — 현재 sprite와 동일한
        정체성 유지: 같은 RobotSpec·MBTI·변주 시드)
     + scene_en + "no text, no letters, no words in the image"
     → sprite::generate() — 텍스트 호출 성공 후에만 (과금 순서 가드)
     ※ 컷은 배경 투명화(make_background_transparent) 미적용 — 장면 전체가 그림
  ⑥ 성공 시에만: daily_cut.png 원자적 교체(최신 1장) + 메타 갱신
     { date, attempts, caption, shot } + emit 'daily_cut:ready'
     실패 시: attempts+1만 기록, 기존 컷 파일 유지
```

### 저장·메타

| 파일 | 내용 |
|---|---|
| `app_data/daily_cut.png` | 최신 컷 1장 (성공 시에만 덮어쓰기) |
| `app_data/daily_cut.json` | `{ date: "YYYY-MM-DD", attempts: u8, caption: String, shot: String }` — 멱등 키 + 재시도 카운터 + 캡션. 손상 시 초기화(리셋) |

### 프론트엔드

- `api.ts`: `getDailyCut(): Promise<{ png: string; caption: string; date: string } | null>`
- 신규 `get_daily_cut` 커맨드 — **읽기 전용** (생성은 파이프라인만, 과금 가드).
- `RobotPortrait.svelte`: **내 화면(seed 없음)일 때만** 컷 우선 렌더 —
  컷 + 하단 캡션 한 줄 프레임(미니홈피 대문사진 + 감성 글귀 결). 컷 없으면 기존
  sprite → canvas 절차 생성 폴백 체인 그대로. `daily_cut:ready` 이벤트 listen.
- 프레임 비주얼 디테일(폴라로이드 스타일 등)은 구현 시 frontend-design 스킬 참고.

### 설정

- `daily_cut_enabled` — `set_setting` allowlist(`commands.rs`)에 추가, 기본 off.
- 설정 탭 이미지 모델 섹션 근처에 토글 UI + 과금·전송 수위 안내 문구.

### 에러 처리

| 상황 | 처리 |
|---|---|
| 텍스트 호출 실패 / JSON 파싱 불량 | attempts+1, **이미지 호출 안 감** |
| 이미지 생성 실패 | attempts+1 |
| 3회 소진 | 그날 포기, 기존 컷 표시 유지 |
| 일기 없는 날(활동 없음) | no-op (컷도 그대로) |
| `daily_cut.json` 손상 | 재초기화 후 정상 플로우 |

### ADR 0024 — 이미지 엔진 전송 소재 수위

ADR 0019(전송 경계: 트랜스크립트 원문 비전송, 엔진이 유일한 외부 경계) 위 확장:

- **지금까지**: 이미지 엔진은 합성 로봇 묘사(`character_description`)만 수신.
- **결정**: 일기 파생 **추상 장면 묘사**(고유명사·프로젝트명·코드 식별자 금지
  지시 적용)까지 허용. 기능은 **옵트인(기본 off)**. 트랜스크립트 원문·일기 원문은
  이미지 엔진에 비전송(일기 본문은 이를 생성한 텍스트 엔진에만 재전송).
- **대안(기각)**: 거친 신호만(작업 반영 요구 약화) / 완전 로컬 축만(요구 포기).

---

## 테스트 전략

| 계층 | 항목 |
|---|---|
| cargo (`crates/core`) | `crop_face` 크롭 좌표·출력 128×128 (합성 색 블록 PNG) |
| cargo | 샷 축 pick 결정성(같은 D+시드 → 같은 샷)·가중 분포 |
| cargo | 텍스트 프롬프트 빌더 — 금지 지시·no-text·MBTI 힌트 포함 |
| cargo | 응답 JSON 파싱 (정상/불량) |
| cargo | 리컨실리에이션 판단 순수 함수 — 멱등·상한·날짜 전환 테이블 테스트 |
| npm (Vitest) | `RobotPortrait` 컷+캡션 / 폴백 분기, `GuestbookTab` face 아이콘 사용 |
| 빌드 | `npm run build` + `cargo test` (네이티브 Windows PowerShell, WSL 금지) |

### 사용자 실환경 체크리스트 (PR 본문)

- [ ] G6: 방명록 아바타 육안 비교 — Rust 크롭이 기존 CSS 크롭과 동등한가 (1회)
- [ ] H2: `daily_cut_enabled` on 후 실제 생성 1회 (과금 발생) — 컷·캡션 표시 확인
- [ ] H2: 앱 재실행 시 재생성 없음 (멱등) 확인
- [ ] H2: 토글 off 시 미생성 확인

## 스코프 외

- 과거 컷 다시보기·누적 보관 (후속에서 필요 시 그때부터 누적)
- face 아이콘의 GuestbookTab 외 소비처 확장 (방 점유자 라벨 등)
- 일기 프롬프트 수정 (H2는 별도 호출 — 약결합 유지)
