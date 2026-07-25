---
status: done
archived: 2026-07-25
---

# release-amate.sh 팀 범용화 + 프리플라이트 검증 설계

- **날짜**: 2026-07-25
- **컴포넌트**: a-mate (Pillar 1)
- **범위**: 로컬 릴리스 스크립트 `a-mate/scripts/release-amate.sh`를 팀 소수가 각자 WSL에서 안전하게 실행할 수 있도록 (1) 개인 환경 하드코딩 제거, (2) 변경 이전 프리플라이트 검증 추가, (3) 프리플라이트 로직 단위 테스트, (4) stale 문서 갱신. **CI 미도입** — 빌드·서명·발행 모두 로컬 유지.
- **부모 설계**: [2026-07-22-auto-update-design.md](../../../../design/a-mate/specs/2026-07-22-auto-update-design.md) — updater/서명/릴리스 채널을 확정한 문서. 본 문서는 그 릴리스 스크립트를 팀이 공유하도록 다듬는 후속.

## 배경 / 문제

`release-amate.sh`는 현재 salt.jeong 개인 환경에 묶여 있다.

- `REPO=/home/msaltnet/code/space-a` — 클론 절대경로 하드코딩.
- `WIN_BUILD_WSL=/mnt/c/Users/salt.jeong/...`, `WIN_BUILD_WIN='C:\Users\salt.jeong\...'` — Windows 사용자명 박힘.
- 실행 전 검증이 없어, 다른 팀원이 돌리면 **중간까지 진행된 뒤 실패**(commit·tag만 남고 빌드 실패 등)하기 쉽다. 실제로 발생 가능한 실수: 공용 서명 키 미배치, 릴리스 저장소 미생성, 커밋 안 한 더티 트리, 이미 존재하는 태그 재사용.

또한 배포 대상 공개 저장소 `dev-team-404/a-mate-releases`가 **아직 생성돼 있지 않아** 현재 스크립트 5단계(`gh release create`)가 실패한다.

목표: **팀 소수(모두 WSL 사용)가 각자 clone 위치·Windows 계정과 무관하게, 실수하면 변경 이전에 명확한 안내와 함께 멈추는** 릴리스 스크립트.

## 결정 요약

| 항목 | 결정 |
|------|------|
| 배포 형태 | 완전 로컬 (빌드·서명·발행 모두 개발자 WSL). CI 없음 |
| 서명 키 | 팀 공용 1개(`tauri.conf.json` pubkey와 짝). 각자 `~/.tauri/`에 배치. **키는 로컬 보관** |
| 스크립트 형태 | 기존 bash 유지 (팀 전원 WSL). PowerShell 재작성 없음 |
| 접근안 | **B — 범용화 + 프리플라이트 검증** (설정 파일 분리 C는 YAGNI) |

### 서명 키 공유 원칙 (중요)

updater는 앱에 박힌 **공개키**로 업데이트 서명을 검증한다. 유효한 업데이트를 내려면 **그 공개키와 짝인 동일한 개인키**로 서명해야 한다. 따라서:

- 팀은 **하나의 공용 키페어**를 쓴다. 각자 다른 키를 만들면(암호가 같아도) 기존 설치본의 자동 업데이트가 거부된다.
- 개인키 + 암호는 **out-of-band(1Password 등 안전 채널)** 로 전달받아 각 릴리서 로컬에 배치한다. **커밋 금지**.
- 암호는 개인키 파일을 여는 로컬 잠금일 뿐, 검증에는 개입하지 않는다 — 공유해야 하는 건 **키 파일**이다.

## 스크립트 변경 — 하드코딩 제거

| 항목 | 변경 후 | 비고 |
|------|---------|------|
| `REPO` | `git -C "$(dirname "${BASH_SOURCE[0]}")" rev-parse --show-toplevel` | 스크립트 위치에서 저장소 루트 자동 유도 |
| Windows 빌드 폴더 | `powershell.exe`로 `$env:USERPROFILE` 조회(끝 CR 제거) → `wslpath`로 WSL 경로 변환. 기본 `<USERPROFILE>\amate-build\a-mate` | env `AMATE_WIN_BUILD`(Windows 형식 경로) 하나로 재정의, WSL 형식은 `wslpath`로 파생. NTFS에 rsync 후 빌드하는 구조는 속도상 **유지** |
| `RELEASE_REPO` | 기본 `dev-team-404/a-mate-releases` + env `AMATE_RELEASE_REPO` 오버라이드 | |
| 키 경로 | `~/.tauri/a-mate-updater.{key,pass}` 관례 유지 (env 오버라이드 지원) | `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` env는 기존대로 우선 |

Windows 빌드 폴더 유도 예:

```bash
WIN_HOME_WIN="$(powershell.exe -NoProfile -Command '[Console]::Out.Write($env:USERPROFILE)')"
WIN_HOME_WIN="${WIN_HOME_WIN%$'\r'}"                       # 혹시 남는 CR 제거
WIN_BUILD_WIN="${AMATE_WIN_BUILD:-$WIN_HOME_WIN\\amate-build\\a-mate}"   # 단일 오버라이드(Windows 형식)
WIN_BUILD_WSL="$(wslpath "$WIN_BUILD_WIN")"               # WSL 형식은 항상 파생
```

## 프리플라이트 검증 (신규)

**모든 변경(버전 갱신·commit·tag·빌드·발행) 이전에** 실행한다. 실패하면 **정확한 해결 명령을 출력하고 즉시 비영이 아닌 코드로 중단** → half-done 상태 방지.

| # | 검사 | 실패 시 동작 |
|---|------|--------------|
| 1 | WSL 환경 (`/proc/version`에 `microsoft`, `powershell.exe` 존재) | 중단 — "WSL에서 실행하세요" |
| 2 | 버전 형식 `^[0-9]+\.[0-9]+\.[0-9]+$` | 중단 — 형식 안내 |
| 3 | 서명 키·암호 존재 (`~/.tauri/a-mate-updater.key` + 암호 해석) | 중단 — 공용 키 out-of-band 수령 후 배치 안내 |
| 4 | `gh` 인증 (`gh auth status`) | 중단 — `gh auth login` 안내 |
| 5 | 릴리스 repo 존재 (`gh repo view $RELEASE_REPO`) | 중단 — 최초 1회 `gh repo create $RELEASE_REPO --public` 안내 (+ updater가 익명 fetch하므로 **반드시 public**) |
| 6 | 워킹트리 clean (`git status --porcelain` 비어 있음) | 중단 — 커밋/스태시 안내 (스크립트가 commit하므로) |
| 7 | 태그 `v$VERSION` 로컬·원격 미존재 | 중단 — 다른 버전 사용 안내 |
| 8 | 현재 브랜치 = `main` | **경고만** (중단 안 함) |

추가: **`--dry-run`** — 프리플라이트만 돌리고, 유도된 경로·릴리스 대상·예상 산출물을 출력한 뒤 버전 갱신·빌드·발행은 건너뛴다. 팀원 첫 사용/점검용.

## 단위 테스트 가능하도록 구조화

프리플라이트를 **개별 함수**로 분리하고, 메인 실행 흐름을 `BASH_SOURCE` 가드로 감싼다:

```bash
check_wsl()          { ... }
check_version_fmt()  { ... }   # $1 = version
check_signing_key()  { ... }
check_gh_auth()      { ... }
check_release_repo() { ... }
check_clean_tree()   { ... }
check_tag_absent()   { ... }   # $1 = version
warn_branch()        { ... }   # 경고만, 항상 0
preflight()          { ... }   # 위 검사 순차 호출

main() { preflight "$VERSION"; ...(mutating steps)... }
if [[ "${BASH_SOURCE[0]}" == "${0}" ]]; then main "$@"; fi
```

- 테스트는 스크립트를 **source**해 각 `check_*` 함수를 직접 호출하거나, `--dry-run`으로 전체를 돌려 종료 코드·출력을 검증한다.
- `git`·`gh`·`powershell.exe`는 **PATH stub**으로, 키 파일은 임시 `HOME`으로 대체해 각 분기(성공/실패)를 격리 검증한다.

## 스크립트 밖 1회성 (문서화, 스크립트 로직엔 미포함)

- **공개 릴리스 repo 최초 생성**: `gh repo create dev-team-404/a-mate-releases --public` (부트스트랩 담당 1인). 스크립트는 없으면 검사 5에서 감지·안내만.
- **공용 키 배포**: 서명 개인키 + 암호를 안전 채널로 각 릴리서에게 전달 → `~/.tauri/a-mate-updater.{key,pass}` (chmod 600). 모두 **같은 키**.

## 문서 갱신

- `docs/design/a-mate/build-and-run.md` "배포" 섹션의 stale 표(*코드 서명 없음 / 자동 업데이트 없음 / CI 없음*)를 현재 실제(updater·서명 연결됨, `release-amate.sh <version>`로 발행, 공용 키 팀 플로우, 1회성 셋업, `--dry-run`)로 교체. `npm run tauri build` 단독(그냥 exe 전달)은 폴백으로 유지.
- Windows 코드 서명 인증서 부재 → 최초 설치 1회 SmartScreen 경고는 부모 설계대로 유효하므로 그 서술은 유지.

## 테스트 / 검증

> ⚠️ **제약**: 이 WSL 세션에서는 a-mate를 빌드/테스트할 수 없다(Rust MSVC 네이티브 Windows 빌드). 실제 빌드·서명·발행 E2E는 여기서 검증 불가.

| 유형 | 내용 | 어디서 |
|------|------|--------|
| 구문 | `bash -n`, `shellcheck` | 이 세션 가능 |
| 프리플라이트 단위 | stub(`git`/`gh`/`powershell.exe`) + 임시 `HOME`으로 각 `check_*` 성공/실패 분기, `--dry-run`이 mutating 단계를 건너뜀을 검증 | 이 세션 가능 |
| E2E (필수·수동) | salt.jeong 환경에서 `--dry-run` 후 실제 릴리스 발행, 설치본이 감지·업데이트되는지 확인 | Windows(WSL) 수동 |

## 범위 밖 (YAGNI)

- CI 기반 자동 릴리스 (GitHub Actions) — 로컬로 확정
- 설정 파일 분리(`release.config`) — 소규모 팀엔 과함
- 키 로테이션 자동화, PowerShell 재작성, macOS/Linux 타깃
- 버전 자동 증가(semver bump 계산) — 버전은 인자로 명시

## 후속 문서화 (프로젝트 규칙)

- **ADR 불필요**: 릴리스 채널·공개 저장소 분리라는 되돌리기 어려운 결정은 부모 설계(2026-07-22)의 후속 ADR 항목이 이미 커버한다. 본 문서의 변경(스크립트 범용화·검증 추가)은 **되돌리기 쉬운 프로세스 개선**이라 새 ADR 대상이 아니다.
- 구현 plan 완료 시 `docs-archive`로 본 spec·plan을 아카이브(DoD).
