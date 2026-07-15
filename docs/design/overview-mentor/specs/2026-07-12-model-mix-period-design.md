# 홈 모델 분포 기간 선택 설계 스펙

- 작성: 2026-07-12 (브레인스토밍 산출물)
- 배경: 홈 "모델 분포"(ModelMix) 위젯이 오늘 하루로 고정 — 주간·월간·전체 관점이 없어
  모델 사용 습관 변화를 볼 수 없다. 백로그 ① (roadmap).
- UI는 **세그먼트 토글**(오늘/주간/월간/전체, 위젯 안 인라인 pill)로 사전 확정. 탭 아님.
- 다음 단계: writing-plans → TDD 구현 → push+PR(base=main, 브랜치 `feat/model-mix-period`)

## 1. 결정 사항 (브레인스토밍 논점 5건)

1. **주간/월간 = rolling**: 주간은 오늘 포함 최근 7일(today−6 ~ today), 월간은 최근 30일(today−29 ~ today).
   calendar(이번 주/달) 기각 — "최근" 직관이 단순하고, `WeekTrend`도 이미 rolling 7일이라 일관.
2. **전체 = 날짜 하한 없음**: SQL에서 from 조건을 생략(`earliest_session_ts` 조회 불필요 — 더 단순, 결과 동일).
3. **기간 상태는 ModelMix.svelte가 자체 소유**: period·mix 상태, fetch, `scan:done` 재로드 구독까지
   전부 위젯 안(self-contained). HomeTab의 mix 로딩 코드는 제거.
4. **백엔드는 기존 `get_model_mix`에 `period` 파라미터 추가** (새 커맨드 기각 — 중복,
   from/to 클라이언트 전달 기각 — "오늘"의 로컬 타임존 정의가 프론트/백엔드로 이중화됨).
   범위 집계는 store에 `model_mix_for_range` 추가.
5. **host 필터**: 현행 `model_mix_for_date`에 host 조건이 원래 없음(이미 전 호스트 합산) —
   범위 쿼리도 동일하게 무필터. 추가 작업 없음.

## 2. 변경 상세

### 2a. store — `crates/core/src/store.rs`

```rust
/// 기간 내 모델별(raw id, 구 데이터 family 폴백) 토큰(입력+출력) 합. 내림차순.
/// from=None이면 하한 없음(전체). 날짜는 로컬 버킷(date(ts,'localtime')), 양끝 포함.
pub fn model_mix_for_range(&self, from: Option<&str>, to: &str) -> Result<Vec<(String, u64)>>
```

- SQL은 기존 `model_mix_for_date`와 동일 골격에 WHERE만
  `date(ts,'localtime') <= ?2 AND (?1 IS NULL OR date(ts,'localtime') >= ?1)`.
- 기존 `model_mix_for_date(date)`는 `model_mix_for_range(Some(date), date)` 위임 래퍼로 축소
  (SQL 중복 제거, 기존 테스트 무수정 통과).

### 2b. 커맨드 — `src-tauri/src/commands.rs`

```rust
/// period("today"|"week"|"month"|"all") → (from, to) 로컬 날짜 범위. 알 수 없는 값은 today 취급.
fn period_range(period: &str, today: chrono::NaiveDate) -> (Option<String>, String)

#[tauri::command(async)]
pub fn get_model_mix(state: State<AppState>, period: Option<String>) -> Result<Vec<ModelMixEntry>, String>
```

- `get_model_mix`는 `period.unwrap_or("today")` → `period_range(p, Local::now().date_naive())` →
  `model_mix_for_range(from, to)`. `period_range`는 순수 함수라 today 고정 인자로 단위 테스트.
- 와이어 필드명 `tier`는 **유지** (실제 값은 raw model id — 기존 계약, ModelMix.svelte 주석과 동일).
- `period: Option<String>`이므로 파라미터 없는 기존 호출 형태도 유효(호환).

### 2c. 프론트 API — `src/lib/api.ts`

```ts
export type ModelMixPeriod = 'today' | 'week' | 'month' | 'all';
export const getModelMix = (period: ModelMixPeriod = 'today') =>
  invoke<ModelMixEntry[]>('get_model_mix', { period });
```

### 2d. 위젯 — `src/lib/ui/home/ModelMix.svelte`

- props `{mix}` 제거 → 내부 상태 `period`(기본 `'today'`)·`mix`. 마운트 시 load,
  pill 클릭 시 period 변경+load, `onScanDone` 구독 시 load(구독 해제는 기존 HomeTab 패턴).
- 토글 연타 대비 요청 시퀀스 가드 한 줄(마지막 요청 응답만 반영).
- 헤더: `h3` "모델 분포"(고정) + 우측 세그먼트 pill 4개(오늘/주간/월간/전체) flex row.
  스타일은 기존 토큰만(활성 `--pastel-lav`류, 11px 내외) — 세부는 구현 재량.
- 빈 상태 문구: 오늘 → "아직 오늘 기록이 없어요"(기존 유지), 그 외 → "이 기간엔 기록이 없어요".
- 색·라벨·비율 로직(FAMILY_COLOR, pct 등) 무변경.

### 2e. 홈 탭 — `src/lib/ui/HomeTab.svelte`

- `getModelMix` import, `mix` 상태, `load()`의 mix 항목, `<ModelMix {mix} />` prop 제거 → `<ModelMix />`.
- `ModelMixEntry` type import도 제거(고아 정리).

## 3. 검증 / 테스트 기준 (TDD)

- **core(store)**: `model_mix_for_range` — 경계 포함(from=to 당일), 범위 밖 제외,
  from=None이 과거 전부 포함, 로컬 날짜 버킷 회귀. 기존 `model_mix_for_date` 테스트 무수정 통과(래퍼 검증).
- **app(commands)**: `period_range` 단위 테스트 — today/week(−6)/month(−29)/all(None)/미지의 값→today.
- **front**: `npx vitest run` 회귀 + `npm run build` (컴포넌트 테스트 관례 없음 — 신규 vitest 없음).
- **최종 판정**: 앱 실행 후 토글 4개 육안(기간별 수치 변화·빈 상태 문구).

## 4. 스코프 밖

WeekTrend 등 다른 위젯의 기간 선택, calendar 주/월, host 필터 도입, 기간 선택 영속화(재시작 시 today로 복귀),
daily_rollup 활용 최적화(events 직접 집계 유지), 다이어리·코칭 연동.

## 5. 구현 참고

- 빌드(Windows): 메모리 `build-env` mingw 레시피 필수(매 cargo 전).
- push revocation 에러 시 `git -c http.schannelCheckRevoke=false push`.
- 커밋 태스크 단위, main 직접 커밋 금지.
