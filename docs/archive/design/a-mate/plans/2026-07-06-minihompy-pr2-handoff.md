---
status: superseded
archived: 2026-07-19
---

# 미니홈피 PR②(채팅 탭+triage) — 실행 핸드오프 (새 세션용)

> **목적**: 로컬 메모리가 없는 새 세션에서 PR② SDD 실행을 바로 시작하기 위한 문서.
> 스펙: `docs/specs/2026-07-05-minihompy-restyle-design.md` §5·§7 (사용자 승인 완료)
> 플랜: `docs/plans/2026-07-06-minihompy-pr2-chat-triage.md` (8태스크, 코드 verbatim 포함, 셀프리뷰 완료)

---

## 시작 프롬프트 (새 세션에 그대로 붙여넣기)

```
docs/plans/2026-07-06-minihompy-pr2-chat-triage.md 플랜을 superpowers:subagent-driven-development로 실행해줘.
브랜치 feat/minihompy-pr2-chat은 이미 생성돼 있음(checkout만). 원장은 .superpowers/sdd/progress.md에 새 섹션으로.
시작 전에 docs/plans/2026-07-06-minihompy-pr2-handoff.md(이 문서)를 읽고 빌드 환경 검증(cargo test --workspace + npm run test 베이스라인)부터 할 것.
컨트롤러는 opus, 구현 서브에이전트는 sonnet(플랜에 코드가 verbatim으로 있어 충분), 최종 whole-branch 리뷰만 가능한 최상위 모델로.
```

## 진행 상태 (2026-07-06 기준)

- PR #8(셸)·#9(마스코트)·#10(미니홈피 본체)·#11(코칭 v2) **전부 머지 완료**. main = ed43f9f.
- 브랜치 `feat/minihompy-pr2-chat` 생성됨(ed43f9f에서 분기). **베이스라인 테스트는 아직 안 돌림** — 실행 첫 단계로 수행할 것.
- 이 플랜이 3단계 마지막 PR. updater는 범위 제외(서명 인프라 미확정).

## 플랜의 스펙 밖 결정 4건 (사용자에게 아직 개별 승인 안 받음 — E2E에서 뒤집기 쉬움)

1. **`chat_status` 커맨드 신설**: 스펙 §8 표면에 없음. §5 "엔진 미설정 = 에러 아닌 안내" 구현 수단(프론트가 설정 여부를 알아야 함).
2. **content_protected 트레이 토글 추가**: 스펙은 "설정값을 창에 적용"만 요구하나 이 설정을 바꿀 UI가 전무해 E2E 불가 → 기존 트레이 패턴(마스코트 표시/실시간 조언)으로 스위치 추가. 명백한 소규모 스코프 추가 — 사용자가 반대하면 토글만 빼면 됨(적용 로직은 유지).
3. **채팅 이력 전송 상한 20턴 / 시스템 프롬프트 findings 상한 10건**: 토큰 억제용 임의값.
4. **"오늘" 정책의 선**: 날짜 버킷(rollup·model_mix·findings_for_date·달력·오늘 판정)만 로컬 자정 기준, 절대 시각(finding first/last_seen, last_scan_ts)은 UTC RFC3339 유지.

## 빌드 환경 (원 개발 머신 기준)

Rust가 시스템 PATH에 없고 MSVC 링커도 없어 **GNU 툴체인 + mingw** 사용. Git Bash에서 cargo 실행 전 매번:

```bash
export PATH="$HOME/.cargo/bin:/c/Users/jibin/mingw64/mingw64/bin:$PATH"
export CARGO_HTTP_CHECK_REVOKE=false
export RUSTUP_TOOLCHAIN=stable-x86_64-pc-windows-gnu
```

- cargo를 호출하는 **모든 서브에이전트 프롬프트에 이 블록을 포함**할 것.
- `git push`가 revocation 에러로 실패하면: `git -c http.schannelCheckRevoke=false push`.
- Tauri 크레이트는 GNU에서 `crate-type=["rlib"]`만 가능(이미 설정됨). LLM 엔진 env는 `./.env.ref`(gitignored) 참조 — 없어도 자동 게이트 실행에는 지장 없음(채팅 E2E에만 필요).

## 리포 관례

- **SDD**: 태스크별 fresh 서브에이전트 구현+리뷰 → 원장(`.superpowers/sdd/progress.md`) 기록 → **최종 whole-branch 리뷰는 최상위 모델 1회**(PR #8·#9·#11 모두 이 단계에서 Critical을 잡았음).
- **PR**: 푸시 → PR 생성 → 봇 리뷰(gemini/Codex)는 **코드 대조 검증 → 픽스 → 인라인 답글** → 사용자 수동 E2E가 최종 게이트. E2E 체크리스트는 플랜 말미에 있음.
- **핵심 기술 학습**(재발 방지): store 커맨드는 반드시 `#[tauri::command(async)]` / `generate_handler` 포함 테스트 바이너리는 WebView2Loader 크래시 → 순수 로직은 core에 / muda CheckMenuItem은 클릭 시 checked 자동 토글 — store를 소스오브트루스로 / 외형 피드백은 스크린샷 기반 사용자 E2E로만 판정.

## 플랜 실행 시 주의 (셀프리뷰에서 이미 반영된 결정)

- **락 규율**: `chat_send`는 컨텍스트 수집만 락 안에서, LLM 네트워크 호출은 반드시 락 해제 후 (Task 3 코드에 반영됨).
- **role 주입 차단**: 프론트發 메시지는 user/assistant만 허용 — system은 백엔드만 조립.
- **로컬 "오늘" 전환 시 테스트 픽스처**: 자정 부근 UTC 시각이면 날짜가 밀림 — 기대값을 `chrono::Local` 변환으로 계산하거나 시각을 `T01:00~T10:00Z`로. `ingest_file_then_rollup_aggregates_tokens`의 날짜 리터럴 교체는 플랜 Task 1 Step 4에 명시됨.
- **한글 IME**: 채팅 Enter 전송은 `e.isComposing` 가드 필수 (Task 4 코드에 반영됨).
- **scan:progress는 5건 단위 emit**(마지막은 항상) — 파일 수천 개 이벤트 플러딩 방지.
- 파스텔 토큰만 — `--pastel-coral` 등 토큰명은 `src/lib/theme.css` 실존 여부 확인 후 사용.

## E2E 후

- 사용자 E2E 통과 → superpowers:finishing-a-development-branch → PR 생성.
- PR② 머지로 3단계(미니홈피 대개편) 완료. 다음 후보: 코칭 v2.1(`docs/brainstorming/2026-07-06-coaching-v2.1-kickoff.md` — R5 재설계·R11 승격·ToolResult/UserPrompt 수집).
