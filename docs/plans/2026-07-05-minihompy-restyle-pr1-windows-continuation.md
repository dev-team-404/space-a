# 미니홈피 대개편 PR① — Windows 이어가기 핸드오프

> **목적**: PR①(미니홈피 본체)을 macOS에서 **구현 + 자동 게이트까지** 완료했다. 실제
> **시각/런타임 E2E와 거기서 나올 수정은 Windows에서** 이어간다. 이 문서 하나로 Windows
> 머신에서 바로 검증·마무리·머지를 진행할 수 있다.
>
> - 브랜치: `feat/minihompy-restyle` (main @ `a8adea3`에서 분기, 8 커밋)
> - 플랜: `docs/plans/2026-07-05-minihompy-restyle-pr1.md` (9 태스크, 코드 verbatim)
> - 스펙: `docs/specs/2026-07-05-minihompy-restyle-design.md`
> - SDD 원장: `.superpowers/sdd/progress.md` (태스크별 상태·게이트 수치·발견사항 전체)

---

## 1. 지금까지 된 것 (macOS에서 완료·검증)

8개 구현 태스크 전부 완료, 태스크별 fresh 서브에이전트 구현 + 리뷰 통과. **모든 자동 게이트 그린.**

| 태스크 | 내용 | 커밋 |
|---|---|---|
| 1 | core — `coach::fix_command` + findings 상태/모델분포 쿼리 | `3dcebe8` |
| 2 | src-tauri — 커맨드 표면 + 위치 클램핑 + occasion pull 단일화 | `18d68c1` |
| 3 | 프론트 기반 — api.ts·theme.css·notices·RobotPortrait·App.svelte 레이아웃 | `153ff1e` |
| 4 | 홈 탭 — 오늘 스트립 + 위젯 4종 | `988557b` |
| 5 | 마이룸(MiniRoom) | `5119226` |
| 6 | 코칭 탭 재설계 | `d823af1` |
| 7 | 다이어리 탭 — 달력 + 마크다운(marked+DOMPurify) | `f4f3a84` |
| 8 | 마스코트 개편 — 클릭→홈피·지속 말풍선·occasion pull·파스텔 | `05f1273` |

**최종 자동 게이트 (macOS, 2026-07-05):**
- `cargo test --workspace` → core **101 pass** (+1 skip, 아래 §2 참고), app **11 pass**, 무경고
- `cargo build` → 성공
- `npx vitest run` → **37 pass** (6 파일)
- `npm run build` → 성공, **경고 0**
- 신규 의존성: `marked@18.0.5`, `dompurify@3.4.11` (플랜 허용 범위)

---

## 2. 실행 환경 차이 (반드시 이해하고 시작)

이 브랜치는 플랜/핸드오프가 상정한 **Windows + Git Bash + mingw GNU 툴체인**이 아니라
**macOS(darwin, aarch64)** 에서 구현됐다(사용자 결정: 여기선 확정적 부분만 구현+PR, 검증은 Windows).

- Rust 타깃이 `aarch64-apple-darwin`였다(플랜의 `x86_64-pc-windows-gnu` 아님). **Windows에선
  원래 핸드오프의 빌드 블록을 그대로 쓰면 된다** (아래 §3).
- **알려진 Windows 전용 테스트 1개** — `core::hosts::tests::host_source_derives_sibling_paths`.
  이 테스트는 `C:\Users\jibin\...` 경로를 하드코딩해 단언한다. Unix에선 `\`가 경로 구분자가
  아니라 실패하므로 macOS 게이트에선 `--skip`으로 제외했다. **PR① 범위 밖(hosts.rs 안 건드림).
  Windows에선 이 테스트가 정상 통과할 것** — Windows 첫 게이트에서 스킵 없이 돌려 102/102(core)를
  확인하라.
- `svelte-check`는 macOS에서 59개 에러를 뱉지만 **전부 `node_modules/vitest/*.d.ts`** 안의
  `@types/node` 부재 잡음이다(프로젝트 소스 아님, 플랜상 svelte-check는 선택). 실제 소스
  경고는 1건뿐(§5의 line-clamp). Windows에서도 `@types/node`를 안 깔면 동일한 잡음이 나온다 —
  권위 있는 컴파일 게이트는 `npm run build`(=vite+svelte 플러그인)이며 경고 0으로 통과한다.

---

## 3. Windows에서 셋업 + 빌드

Git Bash에서 cargo 호출 전 매번 (원 핸드오프 기준 — mingw 경로는 머신마다 다름):

```bash
export PATH="$HOME/.cargo/bin:/c/Users/jibin/mingw64/mingw64/bin:$PATH"
export CARGO_HTTP_CHECK_REVOKE=false
export RUSTUP_TOOLCHAIN=stable-x86_64-pc-windows-gnu
```

- rusqlite `bundled`가 SQLite를 C 컴파일 → GCC(mingw) 필요. schannel CRL 실패로 크레이트
  다운로드가 막히면 `CARGO_HTTP_CHECK_REVOKE=false`. `git push`가 revocation으로 실패하면
  `git -c http.schannelCheckRevoke=false push`.
- 프론트: `npm install` (macOS에서 이미 package.json/package-lock에 marked·dompurify 반영됨 —
  Windows에선 `npm ci` 또는 `npm install`로 동일 상태 복원).

**Windows 첫 게이트 (스킵 없이):**
```bash
cargo test --workspace          # 기대: core 102, app 11, 무경고 (hosts 테스트 포함 통과)
cargo build
npx vitest run                  # 기대: 37 pass
npm run build                   # 기대: 성공, 경고 0
```

---

## 4. 수동 E2E 체크리스트 (플랜 Task 9 Step 2 — Windows에서 실행)

```bash
npm run tauri dev
```

스펙 §1~§6 대응 (외형·상호작용 판정은 스크린샷 기반 사용자 E2E로만 — 자동 게이트로 끝내지 말 것):

1. **chat 창**: 파스텔 배경 위 흰 프레임, 타이틀바 "{사용자명}님의 미니홈피" + TODAY/TOTAL 숫자.
2. **우측 세로 탭 4개**, 활성 탭 색 연결, 코칭 탭 뱃지 수 = 활성 finding 수.
3. **좌측**: 로봇 초상(내 로봇과 동일 외형), 기분 한마디.
4. **홈**: 스트립 수치 / 주간 추이 7개 바(오늘 강조) / 모델 분포 스택 바 / 절약 top3 클릭 →
   코칭 탭 해당 카드로 스크롤 / 최근 알림.
5. **마이룸**: 로봇 idle 애니, 클릭 → happy 모션+대사, 45초 후 대사 순환.
6. **코칭**: 카드 3단(제목/왜/어떻게), 📋 → 클립보드 명령, [해결함]/[무시] → 카드 사라짐 +
   "숨긴 항목 N개", 복원 동작, 절약가능 합계가 숨김 반영.
7. **다이어리**: 달력 도트, 날짜 클릭 → 마크다운 렌더 본문(`##` 등 원시 노출 없음), 월 이동.
8. **마스코트**: 드래그 이동(4px 이상), 짧은 클릭 → chat 창 홈 탭, 재시작 시 위치 복원
   (모니터 밖 좌표는 우하단 폴백).
9. **말풍선**: 자동으로 안 사라짐, X로 닫힘, 본문 클릭 → 관련 탭, 스캔해도 "수집 완료" 류 대사
   없음, 파스텔 둥근 스타일.
10. **트레이 "지금 스캔"** → 코칭·홈 갱신 (기존 무회귀).

---

## 5. Windows에서 결정/수정할 발견사항 (리뷰에서 확정된 것)

모두 **플랜 verbatim 코드에서 비롯**됐고 자동 게이트는 통과한다. 우선순위 순. 상세는 원장
`.superpowers/sdd/progress.md`의 "Important findings" / "Minor findings roll-up" 참고.

> **일부는 최종 리뷰 직후 macOS에서 이미 수정됨** (커밋 `32fe07c`, 결정론적·자동게이트로 검증
> 가능한 것만): ① 버튼 없는 hover 드래그 방어(`e.buttons` 가드), ② 다이어리 stale-resolve
> 가드, ③ `model_mix` "tier"→"family" 주석·테스트명 정정. 아래 목록에서 [✅ 32fe07c]로 표시.
> 나머지(시각·런타임 판정 필요, 또는 플랜 설계 결정)는 Windows로 이관.

### 5.1 결정 필요 (Important — 플랜에서 유래, 사람이 판정)

- **★ 마스코트 말풍선 오버플로 (`MiniRoom.svelte` `.bubble`, 스크린샷 확인 강력 후보)**:
  `.bubble`에 오버플로/줄임 처리가 없는데 `advice`는 짧은 라벨이 아니라 **완결형 한국어 문장**
  (`diary::finding_advice` 산출)이라 2줄 이상 감기며 128px 로봇을 가릴 수 있다. 같은 필드를
  `SaveTop3`는 ellipsis 처리함. **E2E에서 실제로 확인** → 필요 시 `.bubble`에 `overflow:hidden`
  +ellipsis 또는 `-webkit-line-clamp` 추가.
- **마스코트 a11y (`Mascot.svelte` 포인터 핸들러 div)**: `data-tauri-drag-region` 제거 + 수동
  포인터 핸들러로 `a11y_no_static_element_interactions` 경고 → 현재 `<!-- svelte-ignore -->`로
  억제(빌드 경고 0 유지). 클릭→홈 열기가 키보드/AT에서 안 보임. 리뷰어 권장 정식 수정(~4줄):
  `role="button" tabindex="0"` + `onkeydown`(Enter/Space → `openChatTab('home')`). **결정**:
  억제 유지(크롬리스 오버레이 + chat 창에 동일 경로 있음 → 방어 가능) vs 4줄 정식 수정. a11y를
  실제로 테스트할 수 있는 Windows에서 판단.
- **동시 `expand()` 레이스 (`Mascot.svelte:26-52`, 플랜 설계 갭)**: 플랜 신설계가 옛 `pump()`
  직렬화를 제거해, `closeBubble`의 `expand(false)`가 IPC 대기 중일 때 다른 트리거
  (`onNewFindings`/`onScanDone`→`pullOccasions`/잡담 타이머)가 `showBubble`을 부르면 `expand(true)`
  가 겹쳐 `win.setSize`/`setPosition`이 교차 → 창이 잠깐 잘못된 크기/위치로 흔들릴 수 있다.
  **최종 리뷰가 추가로 짚은 뉘앙스**: 항상 자가 교정되진 않는다 — 레이스 쌍이 끝난 뒤 말풍선이
  정상 종료되면 그 어긋난 기준 위치가 `onMoved`로 **저장**돼(그 시점엔 `bubble!==null` 가드가
  더는 억제 안 함) 재시작 후에도 남을 수 있다. 완화책 존재(잡담은 `bubble===null` 확인,
  `sanitize_pos`가 화면 밖 저장 방지, 사용자가 드래그로 재배치 가능) → 최악은 창이 엉뚱한 곳에
  파킹. Windows에서만 관측 가능. **E2E에서 빠른 말풍선 전환 시 드리프트가 보이면** `expand()`를
  ~5줄 promise 뮤텍스로 직렬화. 안 보이면 유지.

### 5.2 배치 결정 (Minor — 플랜 verbatim, 대개 수용 가능)

- **off-token 색/radius 일괄**: 플랜 verbatim CSS가 토큰을 우회하는 값들을 씀 — radius
  `999px`(App `.badge`, ModelMix `.stack`/칩), `4px`(WeekTrend `.bar`), `3px`(ModelMix `.chip`);
  색 `#e9e3f8`(MiniRoom 방 그라디언트), `#fff`(DiaryTab `.sel` 텍스트), `#f4f1fa`(CoachTab `pre`).
  전부 장식용. **결정**: 장식 예외로 수용 vs `--radius-pill`/`--radius-xs`/`--on-accent` 등
  토큰 신설로 통일. (조각조각 고치면 플랜 verbatim에서 크게 벗어나 보류함.)
- **`Mascot.svelte:196` line-clamp**: `-webkit-line-clamp:3` 옆에 표준 `line-clamp:3` 미정의 →
  svelte-check 경고 1건(빌드는 0). 1줄 추가로 해소.
- **`WeekTrend.svelte`**: 빈 상태 분기 없음(형제 위젯엔 있음). IPC 실패 시에만 노출(백엔드는 항상
  7일 반환). 저영향.
- **`MiniRoom.svelte:35` `poke()` setTimeout 미추적**: 탭 전환 시 destroy돼도 무해한 no-op(다른
  효과는 이미 teardown). 빠른 연타 시 happy 창 짧아짐. 무해 확인됨.
- **`Mascot.svelte:69-84` `lastAdviceKey` 비영속**: 창 리로드 시 초기화 → 미해결 top finding의
  advice가 리로드 후 1회 다시 뜰 수 있음. 엣지, 저영향.

### 5.3 확정된 플랜 오타 수정 (조치 완료 — 참고용)

- **`model_mix_for_date`**: 플랜 verbatim SQL은 `model_tier`로 그룹했으나, 플랜 자체 테스트가
  `"opus"/"haiku"` 라벨을 기대하고 다운스트림 `ModelMix.svelte`가 `opus/sonnet/haiku`로 색을
  매핑한다. `model_tier`는 `high|mid|low`, `model_family`가 `opus|sonnet|haiku`. → **`model_family`로
  구현**(구현자·리뷰어·최종리뷰 모두 확인). `ModelMixEntry.tier` 필드는 이름만 tier이고 실제론
  family 라벨을 담음(다운스트림 계약과 일치).

---

## 6. Windows 검증 결과 (2026-07-06, 리뷰 세션)

**자동 게이트 전부 그린 + 회귀 1건 발견·수정(`ea2bdcc`).**

- `cargo test --workspace`: core **102**(hosts 테스트 포함, §2 예측대로 스킵 없이 통과) + app **11**, 무경고
- `npx vitest run`: **37 pass** · `npm run build`: 성공, 경고 0 · `cargo build`: 성공, **앱 실행 스모크 통과**
- **[Windows 전용 회귀, 수정됨]** `cargo test -p agent-mentor-app`이 `STATUS_ENTRYPOINT_NOT_FOUND`(0xc0000139)로
  로드조차 실패 — Task 2(18d68c1)부터. 원인: 테스트 하니스에 muda의 `TaskDialogIndirect`(comctl32 **v6 전용**)
  임포트가 새로 링크됐는데, tauri-build는 매니페스트 리소스를 `rustc-link-arg-bins`(bin 전용)로만 넣어
  테스트 exe에 `.rsrc`(공용 컨트롤 v6 매니페스트)가 없음 → 로더가 comctl32 **5.82**를 잡아 심볼 부재.
  디버그 루프 + 전 임포트 GetProcAddress 검사로 확정. 픽스: build.rs에서 전 타깃 `rustc-link-arg`로
  libresource.a 링크(`rustc-link-arg-tests`는 lib 단위테스트 미적용 — cargo #10937). bin은 동일 아카이브
  중복 전달이나 `.rsrc` 1개로 무해 실측. **mac에선 재현 불가(Windows 로더 전용) — 이런 부류가 §2가 경고한
  실행 환경 차이의 실체.**
- 남은 것: §4 수동 E2E(시각 판정)와 §5.1 결정 3건 — 사용자 몫.
- **PR #10 gemini 리뷰 5건 대응(`f9ef3da`)**: ①CoachTab 복사 성공시에만 '복사됨'(§5.2 해소)
  ②DiaryTab marked 파싱 예외→null 흡수(기존 에러 분기 재사용) ③**MiniRoom 말풍선 2줄 클램프 —
  §5.1 ★ 오버플로 선반영(E2E에선 클램프 잘림 감성만 확인하면 됨)** ④Mascot 표준 line-clamp 병기
  (§5.2 svelte-check 경고 해소) ⑤WeekTrend 날짜 파싱은 거절(타임존 없는 date-time은 ES 스펙상
  로컬 고정 + WebView2 단일 런타임). 인라인 답글 5건 완료. 잔여 §5.1 결정: a11y·expand() 레이스 2건.

## 7. Windows E2E 피드백 1차 반영 (2026-07-06)

사용자 E2E 지적 4건을 같은 브랜치에 반영:

1. **모델 분포가 opus/other 두 덩어리** → `events.model_raw` 컬럼 신설(스키마 마이그레이션 —
   기존 행은 raw 소급 불가라 events/ingest_state/rollup을 비워 **다음 스캔에서 자동 전체 재수집**,
   findings·status·diary는 보존). mix 쿼리는 raw 그룹(구 데이터 family 폴백), 위젯은 모델별
   개별 표시(claude- 접두 제거, 계열 고정색+순환 팔레트, 토큰 수 병기).
   ⚠ **업데이트 후 첫 실행은 풀 재수집이 돌므로 몇 분간 수치가 비어 보일 수 있음.**
2. **홈 스크롤바 + 우측 탭 겹침** → 기본 창 1000×760(min 860×620)으로 상향, `.content`
   margin-right 10px + 전역 얇은 파스텔 스크롤바(theme.css)로 책갈피 탭과 분리.
3. **코칭이 어떤 작업에 대한 건지 모름(R7 반복)** → 세션 스코프 finding에 `session`
   컨텍스트(프로젝트 · MM-DD HH:MM 세션) 동봉(`sessions` 테이블 조인), 카드에 📂 줄 표시.
4. **세션 원본 상세보기** → `core::transcript`(경량 JSONL 파서: user/assistant 텍스트+도구 요약,
   사이드체인·도구결과 노이즈 제외, 2000자/2000엔트리 상한, session_id 경로주입 방지) +
   `get_session_transcript` 커맨드 + 코칭 카드 [세션 상세] 버튼 → 모달(Esc/X 닫기).
   전송 없음 — 로컬 파일 파싱만(프라이버시 경계 유지).

## 8. 마무리 & 다음 (PR②)

- Windows E2E 통과 → §5의 결정사항 반영 커밋 → PR① 머지.
- **PR②(채팅 탭 + triage 잔여)**: 스펙 §5·§7 기준. 진행률 바(상태줄 자리만 잡혀 있음)·로그·로컬
  "오늘"·`content_protected`·`allowScripts` 등. 새 브레인스토밍 없이 `superpowers:writing-plans`
  부터 진행하면 됨(핸드오프 문서 참고).

---

## 부록: 최종 whole-branch 리뷰 (fable) 결과

**판정: Ready to merge — Yes. 머지 블로커 없음.** (8커밋 전체, 104KB diff 리뷰)

리뷰가 독립 검증한 것: occasion pull 단일화가 end-to-end 정합·레이스 안전(Rust 트리에 push
0), Rust↔TS 커맨드 계약 정확, DOMPurify-before-`{@html}`가 유일 주입 경로(우회 없음),
`set_finding_status`/`open_chat_tab` 서버측 화이트리스트, `sanitize_pos` 화면밖 좌표 방어,
status 라이프사이클 계층 일관성. 글로벌 제약(Tauri v2·async 커맨드·에러철학·토큰·한국어 주인화법)
전부 준수. **Critical 0.**

### 최종 리뷰 직후 수정 완료 (커밋 `32fe07c`, macOS 자동게이트 재통과)

- **[Important, NEW·非플랜] 버튼 없는 hover 드래그** — `Mascot.svelte` `onPointerMove` 첫 줄에
  `if (e.buttons === 0) { downAt = null; return; }` 추가. 포인터가 요소를 벗어난 채 버튼을 떼면
  `downAt`가 남아, 이후 버튼 없는 hover가 `win.startDragging()`를 잘못 호출하던 구멍을 막음.
  (플랜 verbatim 코드의 버그였음 — 정식 수정.)
- **[Minor] DiaryTab stale-resolve** — `pick()`의 `await getDiary` 뒤에 `if (selected !== date) return;`.
- **[Minor] "tier"→"family" 잔재** — `store.rs` `model_mix_for_date` 주석 + 테스트명
  `..._groups_by_family`로 정정(와이어 필드 `ModelMixEntry.tier`는 계약이라 유지).

### Windows로 이관 (시각/런타임 판정 필요 또는 플랜 설계 결정 — §5 참고)

- **[Important, 플랜] expand() 레이스** — §5.1 (뮤텍스 후보, 드리프트 지속 뉘앙스 포함).
- **[Important, 플랜] 마스코트 a11y 억제** — §5.1 (role/keyboard 4줄 정식수정 후보).
- **[Minor] MiniRoom 말풍선 오버플로** — §5.1 ★ (E2E 최우선 확인, `-webkit-line-clamp:2` 후보).
- **[Minor] 그 외** — off-token 색/radius 일괄, line-clamp 표준속성, NoticeLog 1사이클 지연,
  `copied`가 클립보드 실패 시에도 표시, `lastAdviceKey` 비영속 등 — 전부 플랜 verbatim/무해,
  tracked debt로 수용 또는 스킨 2차 이터레이션 때 처리. §5.2 참고.

리뷰어 권고: **머지 가능**. 유일하게 "머지 전에 굳이 넣는다면"으로 꼽은 게 위 `e.buttons` 가드
였고 이미 반영됨. 나머지는 tracked debt / Windows E2E 후속으로 적절.
