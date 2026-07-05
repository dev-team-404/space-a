# 미니홈피 대개편 PR① — 실행 핸드오프 (다른 머신용)

> **목적**: 로컬 메모리가 없는 머신/새 세션에서 PR① SDD 실행을 바로 시작하기 위한 문서.
> 스펙: `docs/specs/2026-07-05-minihompy-restyle-design.md` (사용자 승인 완료)
> 플랜: `docs/plans/2026-07-05-minihompy-restyle-pr1.md` (9태스크, 코드 verbatim 포함)

---

## 시작 프롬프트 (새 세션에 그대로 붙여넣기)

```
docs/plans/2026-07-05-minihompy-restyle-pr1.md 플랜을 superpowers:subagent-driven-development로 실행해줘.
브랜치 feat/minihompy-restyle 생성부터 시작. 원장은 .superpowers/sdd/progress.md에 새 섹션으로.
시작 전에 docs/plans/2026-07-05-minihompy-restyle-pr1-handoff.md(이 문서)를 읽고 빌드 환경을 먼저 검증할 것.
```

## 진행 상태 (2026-07-05 기준)

- 1단계(셸+데이터) PR #8, 2단계(마스코트) PR #9 **머지 완료**. main = 스펙(45fba4c)+플랜(e78fb41) 커밋 상태.
- 3단계는 **PR 2개 분할**: ① 미니홈피 본체(이 플랜) ② 채팅 탭+triage 잔여(진행률 바·로그·로컬 "오늘"·content_protected·allowScripts — **PR① 사용자 E2E 후 별도 플랜 작성**).
- updater는 범위 제외(서명 인프라 미확정).

## 빌드 환경 (원 개발 머신 기준 — 새 머신에선 먼저 검증)

Rust가 시스템 PATH에 없고 MSVC 링커도 없어 **GNU 툴체인 + mingw** 사용. Git Bash에서 cargo 실행 전 매번:

```bash
export PATH="$HOME/.cargo/bin:/c/Users/jibin/mingw64/mingw64/bin:$PATH"   # mingw 경로는 머신마다 다름
export CARGO_HTTP_CHECK_REVOKE=false
export RUSTUP_TOOLCHAIN=stable-x86_64-pc-windows-gnu
```

- **Why**: rusqlite `bundled`가 SQLite를 C 컴파일 → GCC(mingw) 필요. schannel CRL 체크 실패로 크레이트 다운로드가 막혀 `CARGO_HTTP_CHECK_REVOKE=false` 필요.
- cargo를 호출하는 **모든 서브에이전트 프롬프트에 이 블록을 포함**할 것.
- `git push`가 revocation 에러로 실패하면: `git -c http.schannelCheckRevoke=false push`.
- 새 머신에서 MSVC가 정상이면 GNU 강제가 불필요할 수 있음 — 단, **Tauri 크레이트는 GNU에서 `crate-type=["rlib"]`만 가능**(cdylib는 ld export ordinal 한계, 이미 rlib으로 설정돼 있음). 시작 시 `cargo test --workspace`(기대: 102 core + 4 app, 무경고)로 검증 후 진행.
- LLM 엔진(다이어리)은 `./.env.ref`(gitignored) 참조 — 없으면 다이어리 생성만 조용히 스킵되며 이 플랜 실행에는 지장 없음.

## 리포 관례 (메모리 이관)

- **경로**: 설계 스펙 `docs/specs/YYYY-MM-DD-<topic>-design.md`, 플랜 `docs/plans/`, 브레인스토밍 시드 `docs/brainstroming/`.
- **SDD 관례**: 태스크별 fresh 서브에이전트 구현 + 리뷰 → 원장(`.superpowers/sdd/progress.md`)에 태스크별 상태·게이트 수치 기록 → **최종 whole-branch 리뷰는 상위 모델(fable급) 1회** — PR #8·#9 모두 이 단계에서 Critical을 잡았음. 구현 서브에이전트는 sonnet, 컨트롤러는 opus면 충분(플랜에 코드가 verbatim으로 있음).
- **PR 관례**: 푸시 → PR 생성 → 봇 리뷰(gemini/Codex)는 **코드 대조 검증 → 픽스 → 인라인 답글** → 사용자 수동 E2E가 최종 게이트.
- **핵심 기술 학습**(재발 방지):
  - store 커맨드는 반드시 `#[tauri::command(async)]` — 동기면 메인 스레드 프리즈.
  - `generate_handler` 포함 테스트 바이너리는 WebView2Loader 링크로 크래시 → 순수 로직은 core에.
  - muda CheckMenuItem은 클릭 시 checked 자동 토글 — 설정 store를 소스오브트루스로.
  - 외형(로봇·스킨) 피드백은 스크린샷 기반 사용자 E2E로만 판정 가능 — 자동 게이트로 끝내지 말 것.

## 플랜 실행 시 주의 (셀프리뷰에서 이미 반영된 결정)

- **occasion은 pull 단일화**: 파이프라인 push emit 제거, `get_today_occasions`(게이트+마킹) + 마스코트 마운트·scan:done pull + `occasion:today` 재방송. 시작 레이스 방어가 목적 — push를 되살리지 말 것.
- **dismissed finding의 악화는 말풍선 침묵**(의도), **자동 부활 없음**(규칙이 과거 데이터를 매 스캔 재평가하므로).
- 파스텔 모던만 — 픽셀 폰트·도트 보더 금지, 색/radius/그림자는 `src/lib/theme.css` 토큰만.

## E2E 후

- 사용자 E2E 통과 → superpowers:finishing-a-development-branch → PR 생성.
- PR②(채팅 탭+triage)는 스펙 §5·§7 기준으로 새로 브레인스토밍 없이 **writing-plans부터** 진행하면 됨.
