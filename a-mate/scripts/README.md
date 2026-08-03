# a-mate/scripts

a-mate(Agent Mentor) 개발·릴리스 보조 스크립트 모음.

## 릴리스 도구

### `release-amate.sh` — 릴리스 발행 (WSL)

버전 갱신 → 서명 빌드 → `latest.json` 생성 → GitHub Release 발행까지 한 번에 수행한다.
팀 소수가 각자 WSL에서 실행할 수 있게 **개인 환경 하드코딩을 제거**하고, 발행 전
**프리플라이트 검증**으로 실수(키 미배치·미인증·태그 중복 등)를 변경 이전에 잡는다.

```bash
# 저장소 루트에서 (WSL)
bash a-mate/scripts/release-amate.sh --dry-run 0.2.0   # 프리플라이트만 점검 (변경 없음)
bash a-mate/scripts/release-amate.sh 0.2.0             # 실제 발행
```

**프리플라이트(발행 이전 실행):** ① WSL 환경 ② 버전 형식 `X.Y.Z` ③ 서명 키·암호 존재
④ `gh` 인증 ⑤ 릴리스 저장소 존재 ⑥ 워킹트리 clean ⑦ 태그 `v<버전>` 미존재(로컬·원격)
→ 하나라도 실패하면 해결 명령을 출력하고 중단. ⑧ 브랜치가 `main`이 아니면 **경고만**.

**오버라이드 env:** `AMATE_RELEASE_REPO`, `AMATE_WIN_BUILD`(Windows 형식 경로),
`AMATE_KEY_FILE`, `AMATE_KEY_PASS_FILE`.

> 최초 1회 셋업(공개 릴리스 저장소 생성·팀 공용 서명 키 배치)과 전체 릴리스 플로우·주의사항은
> [build-and-run.md의 "릴리스" 섹션](../../docs/architecture/a-mate/build-and-run.md#릴리스--자동-업데이트-채널로-발행)을 참고.

### `test-release-amate.sh` — 프리플라이트 단위/통합 테스트

`release-amate.sh`의 검증 로직을 프레임워크 없이 순수 bash로 테스트한다.

```bash
bash a-mate/scripts/test-release-amate.sh    # PASS=<n> FAIL=0 이면 성공, 실패 시 종료코드 1
```

- **빌드/발행은 실행하지 않는다.** `gh`·`powershell.exe`를 PATH stub으로, 저장소는 임시 git
  저장소로, 키·`/proc/version`은 임시 파일로 대체해 각 `check_*` 함수와 `--dry-run`
  경로(무-mutation)만 격리 검증한다.
- 실제 빌드·서명·발행 E2E는 이 방식으로 대체 불가 — 작성자가 Windows/WSL 환경에서 수동 확인.
- `wslpath` 없는(비-WSL) 환경에서는 경로 유도가 필요한 일부 테스트를 skip한다.

## 내부 구조 (유지보수용)

`release-amate.sh`는 **source 가능**하게 작성돼 있다(끝의 `if [[ "${BASH_SOURCE[0]}" == "${0}" ]]`
가드로 직접 실행 시에만 `main` 호출). 테스트 하네스는 이를 source해 함수를 직접 부른다.

| 요소 | 역할 |
|------|------|
| `check_*` (8개) | 개별 프리플라이트 검사. 실패 시 stderr에 원인+해결책 출력, `return 1` |
| `init_paths` | 환경 유도 — `REPO`(git), Windows 빌드 폴더(`powershell.exe`+`wslpath`), 키 암호 |
| `preflight` | `check_*`를 순서대로 호출, 첫 실패에서 중단 |
| `main` | 인자 파싱(`--dry-run`) → `check_wsl` → `init_paths` → `preflight` → 발행 단계 |

## 기타 스크립트 (릴리스와 무관, 일회성)

| 파일 | 용도 |
|------|------|
| `clean-interior-chroma-fringe.ps1` / `measure-interior-anchors.ps1` / `split-interior-turnaround.ps1` | 마스코트·인테리어 에셋(스프라이트) 가공용 PowerShell |
| `reset-diary.py` | 다이어리 재생성을 위한 `diary_index` 삭제 개발 유틸 (Windows) |
