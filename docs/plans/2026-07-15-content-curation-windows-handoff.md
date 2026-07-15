# 콘텐츠 큐레이션(AX 튜터) — 무엇을 추가했나 + Windows 이어하기 핸드오프

> **목적**: `feat/content-curation` 브랜치에서 지금까지 구현한 "AX 튜터"(공식 커리큘럼 기반
> 코칭)가 현재 구조에 어떻게 얹혔는지 설명하고, **Windows에서 이어서 실행·테스트**하는 절차를 준다.
> 설계 배경은 [킥오프](../brainstroming/2026-07-14-content-curation-kickoff.md), 검증된 커리큘럼은
> [catalog.json](../brainstroming/2026-07-14-curriculum-catalog.json) 참고.

---

## 0. 30초 요약

- **무엇**: 기존 앱은 로그로 낭비를 *탐지*(R1·R2·R7…)한다. 여기에 **역량을 *성장*시키는 축**을 추가했다.
  내 로그로 현재 AX 역량 위치를 감지해, **공식 Anthropic/OpenAI 커리큘럼의 바로 다음 단계**만 코칭한다.
- **한 줄 철학**: **커리큘럼 = 지도, 내 로그 = GPS.** 이미 잘하는 건 침묵, 지금 손 뻗으면 닿는 것만.
- **상태**: 백엔드~프론트 **end-to-end 배선 완료**, 실데이터로 작동 확인. 실시간 소식 피드는
  코드 완성됐으나 개발 샌드박스 TLS 제약으로 미검증 — **Windows에서 정상 fetch됨**.

---

## 1. 현재 구조에 무엇을 추가했나 (BEFORE → AFTER)

### 🟥 BEFORE — 동료가 만든 기존 구조

```
파일 감시 → 스캔 파이프라인 (ops.rs)
   ingest → inventory → run_rules          # 로그에서 낭비 탐지 (R1·R2·R7…)
        │ Finding
        ▼
   SQLite: findings 테이블
        │ Tauri 커맨드
        ▼
   프론트 홈탭: 주간추이 / 모델분포 / 절약top3 / 알림
        ✗ "지금 뭘 배워야 하나" 안내 없음
        ✗ 공식 가이드·소식과의 연결 없음
```
**문제**: 낭비 탐지(수비)는 하지만, 사용자를 성장시키는 방향 제시(공격)가 없다.

### 🟩 AFTER — 큐레이션 축 추가 (★ = 신규)

```
스캔 파이프라인 (ops.rs) ─ 기존 재사용
   ingest → inventory → run_rules
   ★ + run_curation()                       # 스캔 편승
        │
        ▼
   ★ detect_profile() (profile.rs)          # 내 로그 → 역량 사다리 위치 = "프론티어"
        │
        ▼
   ★ 스코어링 (content.rs)                   # 내장팁 19개(공식 URL) + 소식
        │                                     #   프론티어 부스트 · 마스터 억제 · 태그 게이트
        ▼
   ★ content_items 테이블 (store.rs)         # + 축 쿨다운(팁 닫으면 2주 조용)
        │ ★ list_content 커맨드
        ▼
   프론트 홈탭: (기존 위젯 4종) + ★ "오늘의 배움" 카드(TipCard)
        ✓ 프론티어 팁 + 공식 가이드 링크
```
**개선**: 로그로 현재 역량을 감지해, 공식 커리큘럼의 바로 다음 단계만 코칭한다.

### 역량 사다리 5레벨 (감지 신호는 전부 기존 수집 데이터)

| Lv | 축(Dimension) | 감지 신호 | 공식 강좌 |
|---|---|---|---|
| 0 | 모델 리터러시 | 모델 믹스(상위/하위 비율) | Claude Code 101 |
| 1 | 컨텍스트 위생 | R1·R2 finding, CLAUDE.md | Claude Code 101 M3 |
| 2 | 스킬 재사용 | 스킬 툴콜 유무 | Intro to Agent Skills |
| 3 | 워크플로 자동화 | R10·R11 finding, hooks | Claude Code in Action |
| 4 | 오케스트레이션 | is_sidechain / sub_agent | Intro to Subagents |

---

## 2. 코드 변경 위치

| 위치 | 변경 | 라인 |
|---|---|---|
| `crates/core/src/profile.rs` | **신규** — 역량 감지·프론티어 산출 | ~250 |
| `crates/core/src/content.rs` | **신규** — 팁 카탈로그·소스 trait·스코어링 | ~340 |
| `crates/core/src/store.rs` | **수정** — content_items 테이블·CRUD·쿨다운 | +110 |
| `crates/core/src/ops.rs` | **수정** — `run_curation()`·`fetch_feed_items()` | +40 |
| `crates/core/src/main.rs` | **수정** — `curate` CLI 서브커맨드 | +35 |
| `src-tauri/src/commands.rs` | **수정** — `list_content`·`set_content_status` | +40 |
| `src-tauri/src/pipeline.rs` | **수정** — `maybe_curate_content` 스캔 편승 | +25 |
| `src-tauri/src/lib.rs` | **수정** — 커맨드 등록 | +2 |
| `src/lib/api.ts` | **수정** — ContentItem 타입·래퍼·이벤트 | +25 |
| `src/lib/ui/home/TipCard.svelte` | **신규** — "오늘의 배움" 카드 | ~95 |
| `src/lib/ui/HomeTab.svelte` | **수정** — TipCard 배선 | +15 |

**핵심 통찰**: 새 앱이 아니라, 기존 파이프라인·저장·표면을 그대로 재사용하고 그 위에
"낭비 탐지" 옆에 "역량 성장" 갈래 하나를 얹은 것. 기존 두뇌에 **공식 커리큘럼이라는 나침반**을 꽂았다.

---

## 3. 지금 적용된 것 / 아직인 것

| 항목 | 상태 |
|---|---|
| 역량 사다리 5레벨 감지 (`detect_profile`) | ✅ 코드·테스트·실데이터 확인 |
| 커리큘럼 팁 19개 (내장 T3, 공식 Academy URL) | ✅ 바이너리 내장, 오프라인 동작 |
| 프론티어 스코어링·마스터 억제·축 쿨다운 | ✅ 코드·테스트 |
| DB persist + Tauri 커맨드 + `content:ready` 이벤트 | ✅ 배선 완료 |
| 프론트 "오늘의 배움" 카드 (TipCard) | ✅ 배선, vitest·tsc 통과 |
| 실시간 소식 피드 (changelog fetch, T1) | ⚠️ 코드 완성 · 개발 샌드박스 TLS로 미fetch → **Windows에서 정상** |
| 사내 소스(T2) / LLM 한국어 요약 | ⬜ 미착수 (킥오프 §How) |

---

## 4. Windows에서 이어하기

> ⚠️ 이 앱은 **네이티브 Windows 데스크톱 앱**이다. 빌드·실행은 반드시 **Windows PowerShell/cmd**에서.
> WSL 안에서 GUI를 돌리지 말 것 (레포 [README](../../README.md) 참고). WSL은 *분석 대상*일 뿐이다.

### 4.1 사전 요구사항 (한 번, README와 동일)

관리자 PowerShell에서:
```powershell
winget install --id OpenJS.NodeJS.LTS -e --accept-package-agreements --accept-source-agreements
winget install --id Rustlang.Rustup   -e --accept-package-agreements --accept-source-agreements
winget install --id Microsoft.EdgeWebView2Runtime -e --accept-package-agreements --accept-source-agreements
winget install --id Microsoft.VisualStudio.2022.BuildTools -e `
  --override "--quiet --wait --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended" `
  --accept-package-agreements --accept-source-agreements
rustup default stable-x86_64-pc-windows-msvc
```
새 터미널에서 `rustc --version`, `node --version` 확인.

### 4.2 이 브랜치 받아서 GUI 실행

```powershell
cd space-a
git fetch origin
git checkout feat/content-curation      # 이 작업 브랜치
npm install
npm run tauri dev                        # Vite(1420) → Rust 빌드 → 앱 실행
```
- 앱이 뜨고 첫 스캔이 돌면 **홈탭 상단에 "오늘의 배움" 카드**가 나타난다.
- 팁은 바이너리 내장이라 네트워크 없이도 뜨고, **changelog 소식은 Windows 인증서로 정상 fetch**된다.
- 팁을 `✕`로 닫으면 그 축이 2주간 조용해진다(쿨다운). 스캔마다(60초 디바운스) 자동 갱신.

### 4.3 GUI 없이 백엔드만 빠르게 확인 (CLI)

GUI를 띄우기 전 튜터 로직만 검증하려면:
```powershell
cargo build --release -p agent-mentor --bin agent-mentor
.\target\release\agent-mentor.exe ingest    # 내 .claude 로그 수집 (많으면 수 분)
.\target\release\agent-mentor.exe curate     # 역량 프로필 + 추천 팁 출력
```
소식 피드까지 보려면 (Windows는 실제 fetch 됨):
```powershell
.\target\release\agent-mentor.exe curate     # curate 안에서 changelog 자동 fetch
```

### 4.4 기대 출력 (WSL 실데이터로 검증한 예)

```
== 역량 프로필 (총 이벤트 27654) ==
  model_literacy   InProgress   하위 16턴 / 상위 11453턴 — 혼용 시작
  context_hygiene  Mastered
  skill_reuse      Mastered     스킬 5회 호출
  automation       NotStarted
  orchestration    NotStarted
  → 프론티어(지금 배울 것): model_literacy
== AX 튜터가 지금 보여줄 것 (노출 4건) ==
  [ 500] model_literacy  잔심부름엔 굳이 상위 모델 아니어도 돼요
  ...
```

---

## 5. 테스트 (전부 통과 확인됨)

```powershell
cargo test -p agent-mentor            # core — 신규 13개 포함 (profile·content·ops·store)
npm test                              # 프론트 vitest
```
> 참고: `hosts::tests::host_source_derives_sibling_paths` 1건은 Windows 경로 하드코딩
> 테스트라 **Windows에선 통과**한다(WSL에서만 실패했던 것).

---

## 6. 알려진 관찰 / 다음 후보

- ⚠️ **최근 가중 부재**: `detect_profile`은 *전체 기간* 이벤트로 계산한다. 방대한 과거가 최근
  행동을 압도해, 오늘부터 습관을 바꿔도 튜터가 한동안 못 알아챌 수 있다. "최근 N일 가중" 또는
  "세션 윈도우"를 넣으면 프롬프트/습관 변화에 빠르게 반응한다. → **1순위 개선 후보**.
- 소식 피드 T1은 changelog만 붙였다. 사내 스킬허브·행사(T2)는 어댑터 추가로 확장(킥오프 §How).
- LLM 한국어 마스코트 보이스 요약(엔진 재사용)은 미착수.
- 팁 카탈로그 19개 → 레벨당 더 늘려 "매일 새로움" 강화 여지.
- `content_items`에 대한 src-tauri 커맨드 단위 테스트는 작성했으나, src-tauri 크레이트 전체
  컴파일은 Linux 시스템 라이브러리(webkit/dbus) 부재로 개발 환경에선 미검증 — **Windows에서 확인**.
```
