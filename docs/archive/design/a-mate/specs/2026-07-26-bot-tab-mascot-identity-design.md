---
status: done
archived: 2026-07-26
---

# 봇 탭 재구성 + 마스코트 아이덴티티 설계

- **날짜**: 2026-07-26
- **컴포넌트**: a-mate (Pillar 1)
- **브랜치**: `feat/bot-tab-mascot-identity` (worktree)
- **관계**: [2026-07-22-profile-and-mbti-mascot-design.md](../../../../design/a-mate/specs/2026-07-22-profile-and-mbti-mascot-design.md)의 일부(이름의 의미·MBTI 저장 시 재생성 동작)를 **갱신·대체**한다. 방문/소셜을 일기·마스코트에 반영하는 후속 아이템은 별도 로드맵 [2026-07-26-life-social-diary-followups-roadmap.md](../../../../design/a-mate/plans/2026-07-26-life-social-diary-followups-roadmap.md)로 분리한다.

## 배경 — 현재 구조 (조사 결과)

- 설정 탭은 4그룹(`연결 / 나 / 공개 / 모양`, `groups.ts`). "나"(id `me`) = `MeGroup.svelte`, 내부는 **개인정보 → 주인 메모리** 순서.
- 개인정보 "이름" 필드 = `user_name` 저장값 = **주인(사람) 이름**. 두 곳에 쓰인다: ① a-hub 방/점유자 표시명(`client.rename`), ② 일부 프롬프트의 `{user}` 자리.
- 마스코트 이미지 생성:
  `(uuid, mbti) → robot_spec_from_profile → RobotSpec(6슬롯) → character_description(spec, uuid) → generate(스타일 레퍼런스 + 묘사문) → 이미지 모델 → PNG` (캐시).
  - `RobotSpec`: antenna(=머리형태)·head·eyes·body(=몸통)·arms(=포즈)·palette(=8색). `extended_traits`(체형3·마감4·부착물6)는 **uuid 해시로만** 결정.
  - MBTI는 `robot_spec_from_profile`에서 4축을 슬롯 부분집합으로 제약(`S/N→머리`, `E/I→눈+팔레트`, `T/F→몸통`, `J/P→팔·포즈`). 16타입이 서로 다른 편향을 가진다.
- **문제 1 (다양성)**: 묘사문이 `(uuid, mbti)`로 100% 결정론 → 재생성해도 매번 같은 프롬프트라 사실상 동일 캐릭터. 차이는 이미지 모델의 미세 노이즈뿐.
- **문제 2 (MBTI 표현)**: `extended_traits`(체형/마감/부착물)는 MBTI를 전혀 안 본다. 축마다 1~2슬롯만 미세 조정.
- 호칭 "주인": `diary/mod.rs`는 `DiaryConfig.honorific` 파라미터(항상 `"주인"` 고정 주입). `chat.rs`·`mascot.rs`는 `"주인"`을 문자열 하드코딩.

## 목표 (이 세션)

1. "나" 탭 → **"봇"**. 섹션 제목 "개인정보" → **"마스코트 정보"**. "이름" → **"마스코트 이름"**, **"호칭"** 필드 신설(기본 `"주인"`).
2. 호칭(`owner_title`)을 **일기·코칭·채팅·오늘의 한마디·잡담·메모리** 전반에 전파(하드코딩 "주인" 대체).
3. **MBTI 저장 시 자동 재생성 제거** + MBTI 필드 옆 안내문.
4. **"마스코트 생성" 섹션 신설**(마스코트 정보와 주인 메모리 사이): 미리보기 + 재생성 + 저장. 저장을 눌러야만 전체 반영. 연결 탭의 "캐릭터 재생성" 버튼 삭제.
5. **마스코트 다양성 개선**: 재생성 변주 시드 + MBTI 외관 확장(S/N 스타일링 무드 등 재미 요소 포함).
6. **MBTI(봇 성향) 문체를 봇의 *모든 발화*에 반영** — 일기·오늘의 한마디·잡담·말풍선·채팅·코칭. (예: T 성향 = 팩트 기반 냉정한 말투)

## 비목표

- 주인 성별 필드(스킵 — 결정됨).
- 방문 로그·자동 방명록·인바운드 방문 말풍선·타인 일기/인테리어 일기 반영(별도 로드맵).
- a-hub 서버가 조직/uuid를 저장·활용(기존 비목표 유지).
- `RobotSpec` 계약 변경(프론트 폴백 렌더러가 소비 — 불변).

## 데이터 (로컬 설정 키)

| 키 | 내용 | 기본 | 비고 |
|---|---|---|---|
| `user_name` | **라벨/의미 변경**: 마스코트 이름 = Life 표시명 | (기존값 유지) | 키·데이터 그대로 재사용, **마이그레이션 없음** |
| `owner_title` | **신설** — 마스코트가 주인을 부르는 호칭 | `"주인"` | 미설정 기존 설치는 `"주인"`으로 동작 |
| `user_mbti` | (기존) MBTI 4글자 또는 빈값 | | |
| `user_org` / `user_uuid` | (기존) | | |

**의미 전환의 결과(명시)**: `user_name`은 계속 a-hub 점유자·방 주인 이름으로 쓰이므로, 이제 **Life 방/미니홈피에 마스코트 이름이 표시**된다("나=봇" 전환의 자연스러운 귀결). **주인(사람)의 식별 데이터는 계속 a-hub로 전송하되 지금은 화면에 노출하지 않는다** — "누구의 봇인지"를 나중에 노출할 수 있도록 데이터 파이프는 유지한다(§F).

## A. "봇" 탭 UI (`groups.ts`, `MeGroup.svelte`)

- `groups.ts`: `me` 라벨 `'나' → '봇'`.
- 섹션 제목 `<h2>개인정보</h2>` → `<h2>마스코트 정보</h2>`. 섹션 상단 hint("a-hub 연결과 마스코트 캐릭터에 쓰입니다…")도 마스코트 정보 맥락으로 소폭 조정.
- 마스코트 정보 필드: **마스코트 이름**(`user_name`) · **호칭**(`owner_title`, 신설) · 조직 · 아이디 · MBTI.
- MBTI 필드 **바로 옆** 안내(짧게): *"MBTI는 마스코트 외관 성향과 말투에 반영돼요. 외관은 아래 '마스코트 생성'에서 재생성·저장해야 실제로 바뀝니다."*
- `saveProfile`: `mbtiChanged → regenerateSprite()` 분기 **삭제**. 하단 `hint2`("저장하면 캐릭터 다시 그림") 삭제. 저장은 값만 저장.
- 섹션 순서: **마스코트 정보 → 마스코트 생성(신규 §B) → 주인 메모리**.

## B. "마스코트 생성" 섹션 + 미리보기/저장 워크플로

**프론트** (`MeGroup.svelte` 신규 `<section>`)
- 미리보기 영역: 최초엔 현재 마스코트(`getSprite`), "재생성" 후엔 candidate 표시.
- **재생성** 버튼 → `mascotPreview` (수십 초, busy 상태). **저장** 버튼 → `mascotCommit` (candidate가 생성된 뒤에만 활성).
- 저장 성공 → 마스코트 즉시 교체(`sprite:ready`) + 서버 업로드. 저장 전에는 전체 미반영.

**백엔드** (`commands.rs`, `sprite.rs`)
- `mascot_preview() -> String(base64)`: **새 변주 시드**로 `generate` → `app_data/sprite.candidate.png`에 저장 → base64 반환. `sprite.png`는 건드리지 않는다.
- `mascot_commit() -> ()`: `sprite.candidate.png` → `sprite.png` 승격, `sprite:ready` emit, 허브 업로드(`upload_cached_mascot`). candidate 없으면 에러.
- 기존 `regenerate_sprite` 커맨드는 삭제(또는 `mascot_commit` 흐름으로 대체). 첫 실행 자동생성 `maybe_generate_sprite`는 **유지**(변주 없이 uuid 결정론으로 첫 캐릭터).
- `api.ts`: `regenerateSprite` 제거, `mascotPreview`/`mascotCommit` 추가.
- `ConnectionGroup.svelte`: "캐릭터 이미지" 섹션의 **"캐릭터 재생성" 버튼 + `regenerate` 함수만 삭제**(URL/키/모델 설정·저장·연결 테스트는 유지).

## C. 다양성 + MBTI 외관 (`sprite.rs`, 일부 `mascot.rs`)

**핵심**: `character_description`에 **변주 시드(variation)** 인자 추가.
- 첫 자동생성(`maybe_generate_sprite`): `variation = uuid` → 안정(결정론 유지).
- 재생성(`mascot_preview`): `variation = 랜덤 nonce` → 매번 다른 후보.

**MBTI 외관 매핑 확장** — 각 시각 축의 *부분집합*을 MBTI 성향으로 정의하고, **변주 시드가 그 안에서 세부를 선택**한다(같은 타입=일관된 무드, 재생성마다 다른 구체 조합).

| 축 | 기존(RobotSpec) | 신규(sprite 묘사문 전용, MBTI+변주) |
|---|---|---|
| **S/N** | 머리 형태 | **전체 스타일링 무드**(재미 요소): S=깔끔·단정·조화·기능적 / N=개성·예술·믹스매치·몽환. + 체형: S 단단·컴팩트 / N 슬렌더·경량 |
| **E/I** | 눈·팔레트 | 자세 개방도: E=개방(손 흔들기·허리) / I=차분(뒤로·앞모음) |
| **T/F** | 몸통 | 마감·색온도: T=차가운 금속(brushed steel·charcoal) / F=따뜻·부드러움(cream·matte white) |
| **J/P** | 팔·포즈 | 자세 대칭성: J=대칭·정돈 / P=비대칭·편안 |
| **악세사리** | (uuid만) | 성향 그룹 테마: 분석가 NxT=유틸(툴벨트·백팩·상태등) · 외교관 NxF=감성(헤드폰·어깨램프·부드러운 발광) · 관리자 SxJ=실용/정돈(툴벨트·상태등·미니멀) · 탐험가 SxP=활동/경쾌(어깨램프·없음·가벼운 백팩) |

- 팔레트·악세사리 **어휘를 소폭 확장**(스타일 레퍼런스 일관성을 해치지 않는 선).
- **설계 노트**: `RobotSpec` 계약(프론트 폴백 렌더러 `robot/render.ts`가 소비)은 **불변**. 위 확장은 전부 sprite 묘사문(서버 이미지) 쪽에서만 이뤄진다.
- **결정론 완화(의도적)**: "같은 사람=항상 같은 캐릭터"는 재생성에서 깨진다(대신 저장으로 확정). 첫 자동생성은 여전히 결정론.

## D. 호칭 전파 (`owner_title` everywhere)

- 설정 `owner_title`(없으면 `"주인"`) 해석은 커맨드/파이프라인 층에서 수행.
- `ChatContext`·`CoachingBrief`에 `honorific` 필드 추가. `DiaryConfig.honorific`에 실제 설정값 주입(현재 항상 `"주인"` 고정 → 설정값으로).
- 프롬프트 치환(하드코딩 `"주인"` + `{user}`(주인 실명) → `{honorific}`):
  - `mascot.rs`: `build_daily_line_prompt`·`build_chatter_prompt`의 `'주인'`+`{user}`, `comic_directives`의 예시 문구 "주인".
  - `chat.rs`: `build_chat_system_prompt`·`build_coaching_prompt`의 `'주인'`+`{user}`, `[이번 주 브리프 — {user}]`, 메모리 섹션 헤더 `"[주인에 대해 기억한 것…]"`.
  - `diary/mod.rs`: 이미 `{honorific}` 파라미터 사용 — 주입값만 설정값으로.
- `{user}`(주인 실명)는 프롬프트에서 완전히 사라진다. `user_name`(마스코트 이름)은 **표시 전용**(프롬프트에 주입하지 않는다 — YAGNI).
- **주의**: `assemble_coaching_brief`는 `user_name`을 OS `USERNAME`에서 읽어 `{user}`로 썼다 → 프롬프트에서 `{honorific}`으로 대체. 필드 자체는 남겨도 무방(프롬프트 미사용).

## E. MBTI(봇 성향) 문체 — 봇의 *모든 발화* (`mascot.rs`, `chat.rs`, `diary/mod.rs`)

봇 성향은 **말투가 나오는 모든 채널**에 일관되게 반영한다. 채널과 프롬프트 빌더:

| 채널 | 빌더 | 반영 방법 |
|---|---|---|
| 일기 | `diary` 프롬프트 | mbti voice 블록 주입 |
| **오늘의 한마디** | `mascot.rs::build_daily_line_prompt` | ctx.mbti → voice 주입 |
| **잡담** | `mascot.rs::build_chatter_prompt` | ctx.mbti → voice 주입 |
| **말풍선** | (오늘의 한마디·잡담에서 파생) | 위 두 프롬프트 반영으로 **자동 커버** |
| 채팅 | `chat.rs::build_chat_system_prompt` | ctx.mbti → voice 주입 |
| 코칭 | `chat.rs::build_coaching_prompt` | brief.mbti → voice 주입 |

- 헬퍼 `mbti_voice_hint(mbti: Option<&str>) -> String`:
  - **T/F**: T=사실·수치에 근거해 **냉정·간결·직설**(감정 완충 최소) / F=공감·따뜻·관계 중심
  - **E/I**: E=활기·감탄사·리액션 / I=차분·사색·절제
  - **S/N**: S=구체·사실·디테일 / N=비유·큰 그림·아이디어
  - **J/P**: J=정돈·결론 중심 / P=유연·개방·여지
  - MBTI 미설정 → 빈 문자열(기존 voice 유지).
- **플러밍**: `mbti`(또는 그 voice 힌트)를 `ChatContext`·`CoachingBrief`에 추가(§D의 `honorific`과 같은 배관). `build_daily_line_prompt`·`build_chatter_prompt`는 `ChatContext`를 받으므로 필드 하나로 한마디·잡담·말풍선·채팅이 동시 커버된다. 일기는 `DiaryConfig`.
- **페르소나와의 관계**: 기본 페르소나(1인칭·능청·다마고치)는 **유지**하되 MBTI가 *어떻게 능청떠는지*를 좌우한다. 예) 같은 잔소리라도 **T형은 "세션 5건. 효율 떨어지니 그만"**처럼 팩트로 냉정하게, **F형은 "무리하지 마 주인아, 걱정돼서"**처럼 따뜻하게. 강한 축(특히 T)에서는 톤이 실제로 눈에 띄게 바뀌어야 한다.

## F. 주인 식별 데이터 전송 (Life)

- 목적: Life에서 "누구의 봇인지" 알 수 있어야 하지만 **지금은 노출하지 않는다**. 데이터만 서버로 넘겨 **후속 a-hub UI에서 노출**할 수 있게 파이프를 유지한다.
- a-hub `register`/`rename` payload에 주인 식별자 포함: `user_uuid`(기존) + `owner_os_user`(OS `USERNAME` — `stable_identity` 재사용). **별도 입력 필드는 두지 않는다**("완전 전환" UI 유지).
- Life 방문자 화면·점유자 라벨은 지금은 **마스코트 이름만** 표시한다.
- 서버가 모르는 필드는 무시(무해) — 기존 프로필 스펙의 `org`·`user_uuid` 전송과 동일 패턴. **서버의 저장·노출은 a-hub 후속(비목표)**. 필요 시 `contracts/`의 register 스키마에 optional 필드로 문서화.

## 커맨드 요약

| 커맨드 | 변경 |
|---|---|
| `profile_get` / `profile_set` | `owner_title` 추가(get 반환·set 저장·검증) |
| `mascot_preview() -> String` | **신설** — candidate 생성, base64 반환, sprite.png 미변경 |
| `mascot_commit() -> ()` | **신설** — candidate 승격 + emit + 업로드 |
| `regenerate_sprite` | **삭제** |

## 테스트

- **프론트(Vitest)**: 탭 라벨 `'봇'`; 마스코트 생성 섹션 렌더; MBTI 저장 시 재생성 미호출; 저장 전 candidate 미반영(commit 전 sprite 미변경).
- **Rust(cargo)**:
  - `owner_title` 설정 왕복·기본값 `"주인"`.
  - `character_description(variation)` 결정론(같은 variation→같은 묘사) + 다른 variation→다를 수 있음.
  - MBTI 그룹별 finish/build/pose/accessory 부분집합 소속(표본 다수 uuid).
  - 호칭 치환: 커스텀 호칭이 diary/chat/mascot 프롬프트에 등장하고, 호칭≠"주인"일 때 "주인" 하드코딩 부재.
  - `mbti_voice_hint`: 각 축 문구 포함, 미설정 시 빈 문자열.
  - MBTI voice가 **모든 발화 프롬프트**에 실린다: `build_daily_line_prompt`·`build_chatter_prompt`·`build_chat_system_prompt`·`build_coaching_prompt`·일기 프롬프트에 설정 MBTI의 힌트 문구가 등장(T형 표본에서 "냉정/직설/사실" 계열 문구 확인).

## 마이그레이션 / 호환

- `user_name` 데이터 유지(라벨만 변경). `owner_title` 미설정 기존 설치 → `"주인"`으로 동작(무변화).
- `sprite.candidate.png`는 임시 캐시. commit 시 소비, 미commit이면 다음 preview가 덮어쓴다.

## 파일 터치 (구현 계획 근거)

- **프론트**: `src/lib/ui/settings/groups.ts`, `MeGroup.svelte`, `ConnectionGroup.svelte`, `src/lib/api.ts`
- **Rust**: `src-tauri/src/commands.rs`(profile+owner_title, mascot_preview/commit, honorific 해석), `src-tauri/src/pipeline.rs`(maybe_generate_sprite variation=uuid), `crates/core/src/sprite.rs`(character_description variation + MBTI traits), `crates/core/src/mascot.rs`(honorific + mbti voice: 한마디·잡담·말풍선), `crates/core/src/chat.rs`(honorific + mbti voice + ctx 필드), `crates/core/src/diary/mod.rs`(honorific 주입 + mbti voice)
