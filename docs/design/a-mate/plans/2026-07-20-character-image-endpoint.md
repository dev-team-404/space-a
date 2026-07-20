# 캐릭터 이미지 엔드포인트 분리 + 재생성 (2026-07-20)

## 배경 — 왜 캐릭터가 "기본 그림"으로만 보였나

마스코트는 두 경로로 그려진다 ([`Mascot.svelte`](../../../../a-mate/src/Mascot.svelte)):

| 경로 | 조건 | 결과 |
|---|---|---|
| **AI 스프라이트** | `app_data/sprite.png` 있음 | 레퍼런스 화풍의 고유 캐릭터 (의도된 모습) |
| 절차 생성 폴백 | 없음 | `lib/robot/render.ts`가 코드로 그린 픽셀 캐릭터 |

`sprite.rs`는 사용자가 제공한 레퍼런스(`assets/sprite-style-ref.jpg`)를 **스타일 앵커로 첨부**해
이미지 모델에 "EXACT same art style"로 시드 기반 인물을 그리게 한다. 그런데 `SpriteConfig`가
**텍스트 엔진 설정(`AGENT_MENTOR_ENGINE_URL`)을 그대로 재사용**하는 구조였다.

사내 기본 엔진은 LM Studio(gemma 등 **텍스트 전용**)라, 이미지 생성 요청이 매번 실패했다:

```
[WARN] AI 스프라이트 생성 실패(다음 스캔 재시도): sprite 응답에 이미지 없음
```

→ `sprite.png`가 끝내 생기지 않고 **항상 절차 생성 폴백**만 보였다. 폴백 그림을 아무리 손봐도
근본 해결이 아니다 — 필요한 건 **이미지 모델을 텍스트 엔진과 분리해 연결하는 것**이다.

## 무엇을 바꿨나

### 1. 이미지 전용 설정 (텍스트 엔진과 분리)

`SpriteConfig::resolve(stored_url, stored_key, stored_model)` 신설. 우선순위:

1. 설정창 저장값 — `image_url` / `image_key` / `image_model`
2. env — `AGENT_MENTOR_IMAGE_URL` / `_KEY` / `_MODEL`
3. env 폴백 — `AGENT_MENTOR_ENGINE_URL` / `_KEY` (기존 동작 호환)

URL이 비면 `None` → 스프라이트 기능 전체 no-op(폴백 유지). `AGENT_MENTOR_SPRITE=off`도 그대로.

> 이로써 **텍스트는 사내 LM Studio, 이미지는 OpenRouter** 처럼 갈라 쓸 수 있다.

### 2. 설정창 "캐릭터 이미지" 섹션 + 재생성 버튼

- 입력: 엔드포인트 URL · API 키(password) · 이미지 모델
- `[저장]` → `image_settings_set`
- `[캐릭터 재생성]` → `regenerate_sprite` (락 밖 네트워크, 완료 시 `sprite:ready` emit → 마스코트 즉시 교체)

### 3. 생성 이미지의 흰 배경 → 투명 (버그 수정)

프롬프트가 `plain white background`를 요구하므로 모델 결과는 **흰 배경 불투명 PNG**다.
마스코트 창은 투명이라 그대로 쓰면 캐릭터 주위에 **흰 박스**가 보인다.

`make_background_transparent()` 추가 — **테두리에서 연결된 배경색만** 플러드 필로 알파 0 처리한다.
안쪽 흰색(신발·후드 끈)은 실루엣 아웃라인에 둘러싸여 있어 **보존**된다. 실패해도 원본을 그대로
쓰므로 무해(캐릭터는 어떻게든 보인다).

### 4. 적용 범위 통일

`maybe_generate_sprite`(파이프라인 자동 생성)와 `request_occupant_sprite`(방 점유자)도
env 전용이 아닌 **동일한 설정 해석**을 쓰도록 맞췄다.

## 검증

| 항목 | 결과 |
|---|---|
| 테스트 | core 324 + app 29 통과 (신규: 설정 해석 우선순위, 배경 투명화) |
| 실제 자동 생성 | `[INFO] AI 스프라이트 생성 완료` — 설정창 값으로 앱이 스스로 생성 |
| 투명 배경 | 결과 PNG `mode=RGBA`, 네 모서리 alpha=0 |
| 회귀 | 이미지 설정 미설정 시 기존과 동일하게 폴백 동작 |

## 운영 메모

- 이미지 모델은 **사람당 최초 1회**만 호출하고 `sprite.png`로 캐시한다 — 이후에는 오프라인.
- 키는 로컬 설정(DB) 또는 `.env`에만 둔다. **커밋 금지**(`.env`는 `.gitignore` 대상).
- 사내망에서 OpenRouter(`https://openrouter.ai/api/v1`)는 프록시를 통해 접근 가능함을 확인했다.
