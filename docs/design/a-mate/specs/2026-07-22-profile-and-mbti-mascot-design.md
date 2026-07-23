# 개인정보 프로필 + MBTI 마스코트 설계

- **날짜**: 2026-07-22
- **컴포넌트**: a-mate (Pillar 1)
- **범위**: a-mate 우선. a-hub는 이미 마스코트 PNG 업로드(#85)를 지원하므로 이미지 전송은 그 파이프라인 재사용.

## 목표

1. 미니홈피 **설정 탭(`LifeSettingsTab`)**에 **개인정보** 섹션 — 이름 · 조직(기본 `S/W 혁신팀`) · 아이디(UUID 자동부여) · MBTI(선택).
2. **MBTI로 마스코트 로봇 특징 결정** — 같은 MBTI는 비슷, 개인 UUID로 세부만 다름. MBTI 미설정이면 UUID만으로 결정론 생성.
3. a-hub 연결(register) 시 조직·UUID를 payload에 포함(현재 a-hub는 모르는 필드 무시 → 무해, 추후 확장 대비). 이미지는 기존 #85 업로드가 처리.

## 데이터 (로컬 설정 키)

| 키 | 내용 | 편집 |
|---|---|---|
| `user_name` | 이름 | 사용자 |
| `user_org` | 조직 (기본 `S/W 혁신팀`) | 사용자 |
| `user_uuid` | 아이디 — UUID v4, **최초 1회 생성 후 고정** | 자동(읽기 전용) |
| `user_mbti` | MBTI 4글자 대문자 또는 빈값 | 사용자(선택) |

## 시드 → 로봇 (핵심)

`RobotSpec`(6 슬롯: antenna·head·eyes·body·arms·palette)을 **UUID 해시**로 채우되, MBTI가 있으면 4축을 슬롯 부분집합으로 제약한다. UUID는 그 부분집합 안에서 정확한 선택을 담당.

| MBTI 축 | 반영 슬롯 | 매핑 |
|---|---|---|
| **S / N** | head | S(실용)=각진 {1,5,4} · N(추상)=둥근 {0,2,3} |
| **E / I** | eyes + palette | E=생기 {1,3,5} 눈 · 밝은 액센트 팔레트 {0,2,3,5} / I=차분 {0,2,4} 눈 · 무광 팔레트 {1,4,6,7} |
| **T / F** | body | T(각진)=아머 {1,5,4} · F(부드)=라운드/패딩 {0,3,2} |
| **J / P** | arms | J(정돈)={0,3,1} · P(여유)={2,4,5} |
| (자유) | antenna | UUID 해시 그대로 |

`character_description(spec, uuid)`는 기존 로직 재사용(로봇 어휘). extended_traits(체형/마감/부착물)도 uuid로 변주.

## 생성·연결 배선

- `regenerate_sprite`·`maybe_generate_sprite`: `stable_identity()` 대신 **프로필(uuid+mbti)** 로 spec/description 생성.
  결과 PNG는 `sprite:ready` → `LifeView`가 `lifeSyncMascotImage()`로 서버 업로드(#85). 즉 **MBTI 재생성 = 서버 자동 반영**.
- `hub_connect`: register payload에 `org`·`user_uuid` 추가(서버 무시 가능), 이름은 프로필 `user_name` 우선.
- `user_name`은 사용자 표시 이름의 **유일한 원본**이다. Space A 서버 설정은 이름을 별도로 입력받지 않는다.
- Life 연결은 항상 `user_name`을 등록·재연결 이름으로 사용하고, 개인정보에서 이름을 변경하면 연결된 Life 서버의 에이전트·방 주인 이름도 함께 변경한다.
- 과거 호환 키 `hub_user`는 새 UI와 판정에 사용하지 않으며, 저장 시에만 같은 값으로 덮어써 기존 설치의 불일치를 수렴시킨다.
- `mascot_seed`(폴백용)는 `user_uuid`로 — 서버 이미지 없을 때만 쓰는 폴백.

## 커맨드

- `profile_get() -> {name, org, uuid, mbti}` — uuid 없으면 생성·저장 후 반환.
- `profile_set(name, org, mbti)` — 검증(mbti 빈값 또는 유효 4글자) 후 저장. uuid는 불변.

## 테스트

- uuid: 최초 생성·이후 고정(멱등), v4 형식.
- mbti 검증: 유효/무효 4글자, 빈값 허용, 대소문자.
- spec_from_profile: 같은 MBTI → 제약 슬롯 동일 그룹, uuid로 세부 다름; MBTI 없으면 `robot_spec_for(uuid)`와 동일.
- 프론트: 저장 왕복, MBTI 변경 시 재생성 안내.

## 비목표

- a-hub가 조직/UUID를 **저장·활용**하는 것은 a-hub API 확장 후속(동료 협의). 지금은 전송만.
- a-lens 표시 반영은 별도.
