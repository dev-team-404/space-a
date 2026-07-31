---
status: done
archived: 2026-07-31
---

# 프로젝트 축 — a-mate가 작업 디렉터리를 함께 발행하고 a-lens가 그걸로 묶는다

> **한 줄**: 지금 a-lens는 문서 주제어로 프로젝트를 *추측*한다. a-mate는 이미 세션의
> 작업 디렉터리를 알고 있으므로, 발행할 때 프로젝트 이름을 같이 실으면 추측이 사실이 된다.

- **브랜치**: `feat/a-mate-project-marker` (기점: `feat/a-lens-collab-graph` = PR #142).
  **PR #142가 먼저 머지된 뒤** main 기준으로 리베이스하거나, base를 #142 브랜치로 걸 것.
- **범위**: a-mate(Pillar 1) + a-lens(Pillar 3) 두 컴포넌트를 **한 브랜치**에서 수정한다.
- 선행 조사·결정은 2026-07-31 세션에서 끝났다. 이 문서가 그 결론이다.

## 0. 왜 하는가

`sw-innov` 실데이터 157문서를 주제어로 군집화해봤더니 프로젝트가 안 갈린다:

| 시도 | 결과 |
|---|---|
| 라벨 전파(유사도 그래프) | 60문서짜리 한 덩어리 + 나머지 흩어짐. 임계를 올리면 전부 단문서 군집 |
| 용어 앵커(빈도 기반) | `이벤트`·`구현`·`복구` 같은 **요약체 상용어가 프로젝트 행세**를 한다 |
| 고유명사 앵커 | 그럭저럭(`a-lens`, `agent/meter`, `playwright/컨텍스트`) — 그래도 추정 |

a-mate 회고문은 LLM이 쓴 2~3문장이라 어휘가 비슷하게 수렴한다. 반면 **cwd는 프로젝트를
정확히 가리킨다.** 사람이 프로젝트를 부르는 이름(`space-a`, `agent-meter`)과도 일치한다.

## 1. 재료는 이미 a-mate 안에 있다 (조사 완료)

| 위치 | 내용 |
|---|---|
| `a-mate/crates/core/src/hosts.rs` `project_identity(host, cwd) -> (key, name)` | cwd → 정규화 키 + **표시 이름 = `path_basename(cwd)`**. WSL 직접 세션과 Windows UNC 경로 세션을 **같은 프로젝트로 통합**까지 해둠 |
| `a-mate/crates/core/src/diary/mod.rs` `git_commits_for(host, cwd, ...)` | 세션 cwd에서 **git을 이미 돌린다**(WSL이면 `wsl -d <distro> -- git`). 저장소 루트를 묻는 것도 같은 방식 |
| `a-mate/src-tauri/src/commands.rs` `SessionCtx`/`session_ctx()` | `cwd`·`git_branch`(`EventKind::SessionMeta`)까지 파싱·저장하고 있다 |

즉 **새로 수집할 것은 없다.** 발행 경로에서 빠지고 있을 뿐이다.

> **정정 (구현 중 확인)** — `StruggleSession.project_id`는 `project_identity`의 키가 **아니다**.
> Claude 로그 디렉터리명(`-home-kimmy-core-space-a`)을 `decode_project_id`로 정규화한 값이라
> 구분자가 전부 `-`로 뭉개져 있어 basename을 되살릴 수 없다(`space-a` → `a`).
> 그래서 `struggle_sessions` 조회에 **`cwd`를 실어 온다**(cwd 없는 옛 세션은 같은
> `(host, project_id)`를 cwd와 함께 기록한 세션에서 빌려온다 — 일기와 같은 규율).

## 2. 실을 자리 — 제목뿐이다

work 허브(`spacea.msalt.net`)는 **재배포 불가**다([[space-a-hub-work-not-deployable]] 메모리).
필드를 늘릴 수 없으므로 기존 응답에 실려 나오는 자리에 얹어야 한다.

| 자리 | REST로 읽히나 | 판정 |
|---|---|---|
| 페이지 **제목**(= `resolve_issue`의 `summary`) | ✅ `/spaces/{id}/tree`, `/pages/{id}` | **여기** |
| 이슈 **제목** | ✅ `/issues` | **여기(이중화)** |
| 페이지 `body` | issue-derived는 **빈 값**(resolve가 body를 안 채운다 — 실측 157건 중 본문 있는 건 14건) | ✗ |
| `steps` | 모델엔 있으나 **REST 응답에 없음** | ✗ |

**기존 관례를 그대로 쓴다.** a-mate는 이미 `[a-mate:R8:github]` 형태의 기계 키를 제목에 심고
(`crates/core/src/hub.rs:361-368`), a-lens는 `_MACHINE_MARKER`(`collector.py`)로 그걸 떼고
표시한다. 같은 문법으로 확장한다:

```
[a-mate:proj=space-a] a-lens Docker 배포 및 방 공유 안정화
```

구버전 a-lens도 이 접두를 **떼고 표시**하므로 화면이 깨지지 않는다(하위 호환 확보).

## 3. 규칙 (결정된 것)

0. **프로젝트 = git 저장소 루트의 이름** (2026-07-31 결정). cwd basename으로는 같은 프로젝트가
   사람마다 갈린다 — `space-a/`에서 일한 사람은 `space-a`, `space-a/a-mate/`에서 일한 사람은
   `a-mate`가 된다. `git rev-parse --path-format=absolute --git-common-dir`로 루트를 찾고
   그 basename을 쓴다. 공용 디렉터리를 보므로 **worktree도 본 저장소로 합쳐진다**.
   서브모듈(`.git/modules/…`)은 합치지 않고 `--show-toplevel`로 폴백한다. git이 없거나
   저장소가 아니면 cwd basename으로 폴백 — best-effort다.
1. **전체 경로 금지, basename만.** 저장소 루트 경로는 `/home/kimmy/core/space-a`처럼 홈 경로를
   품는다. 그대로 보내면 팀 공간에 개인 경로가 남는다 — a-mate 회고 프롬프트도
   "회사·개인 식별 정보는 넣지 않는다"고 못박고 있다. `space-a`만 보낸다.
2. **슬러그 정제**: 소문자, `[a-z0-9._-]`만 남기고 나머지는 `-`, 연속 `-` 축약, 32자 제한.
   비면 마커를 아예 붙이지 않는다(빈 마커 금지).
3. **LLM에 맡기지 않는다.** 프롬프트에 넣으면 모델이 경로를 지어내거나 문장에 섞는다.
   `parse_retro_reply` **뒤에 코드가 결정론으로** 접두를 붙인다.
4. **소급 적용 없음.** 기존 157건에는 마커가 없다. 화면은 "마커 있는 문서(사실)"와
   "없는 문서(주제어 추정)"를 **구분해서** 보여준다 — 섞어서 다 아는 척하지 않는다.

## 4. 할 일

### 4.1 a-mate (Rust)

| 파일 | 작업 | 상태 |
|---|---|---|
| `crates/core/src/store.rs` | `StruggleSession.cwd` 추가 + `struggle_sessions` 조회에 cwd(옛 세션은 같은 프로젝트의 다른 세션에서 보충) | ✅ |
| `crates/core/src/hosts.rs` | `git_repo_root(host, cwd)` — WSL/네이티브 양쪽에서 `git rev-parse`. 순수부 `repo_root_from_common_dir`는 따로 떼어 테스트 | ✅ |
| `crates/core/src/hub.rs` | `project_slug(host, dir)`(순수·§3.2 정제) · `project_marker(slug)` · `retro_project_marker(session)`(git 호출) · `retro_issue_title` · `retro_page_summary`(둘 다 순수) | ✅ |
| 발행 경로 **네 곳** | `src-tauri/src/pipeline.rs`(신규·resume) + `crates/core/src/hub.rs`의 `run_retro_push`·`resume_retro_pendings`(CLI 경로). 마커는 세션당 한 번만 계산해 돌려 쓴다 | ✅ |

- 마커는 `[a-mate 회고]`보다 **앞**에 붙인다: `[a-mate:proj=space-a][a-mate 회고] {title}`.
  a-lens의 기존 제거 규칙이 **문두만** 보기 때문에, 뒤에 두면 구버전 화면에서 마커가 그대로 뜬다.

- 빌드·실행은 **네이티브 Windows PowerShell**에서(`npm run tauri dev`). WSL 금지 — a-mate/CLAUDE.md 제약.
- 허브 푸시는 dev 빌드 + `.env`에서만 켜진다([[a-mate-hub-push-enabled]] 메모리).
- `cargo test`는 WSL에서 돌려도 된다(순수 로직).

### 4.2 a-lens (Python + TS)

| 파일 | 작업 | 상태 |
|---|---|---|
| `backend/alens/collector.py` | 마커 파서 `_parse_project(title) -> (project|None, 남은 제목)`. 페이지·이슈 양쪽에 적용. **번역(LLM) 입력과 표시 제목에서 마커 제거** — 프로젝트는 캐시가 아니라 매번 원제목에서 읽는다(캐시에 굳는 title은 이미 마커를 뗀 것). 로비 피드 서사 문장도 표시 제목으로 교체(원제목을 쓰고 있었다) | ✅ |
| `backend/alens/collab.py` | `_projects()` — **마커 있는 문서만** 프로젝트로 집계(주제어 추정으로 채우지 않는다). `graph.projects = {nodes:[{id, docs, people:[{id, docs}]}], unknown_docs}` | ✅ |
| `frontend/src/api.ts`·`main.ts` | 프로젝트 축 표시. 사실(마커)과 추정(주제어)을 **시각적으로 구분** | ⏳ §4.3 확정 후 |
| `contracts/` | 계약 변경 아님(허브 응답 모양 그대로). 문서만 | — |

### 4.3 화면 (초안 — 구현 전 확정할 것)

- 협업 지도에 **보기 전환**: `사람` ↔ `프로젝트`.
- 프로젝트 보기: 노드 = 프로젝트(크기 = 문서 수), 선 = 두 프로젝트를 같은 사람이 걸침.
  또는 이분 그래프(왼쪽 사람 · 오른쪽 프로젝트).
- 마커 없는 문서는 `(분류 안 됨)` 묶음으로 두고 수를 적는다 — 정직성 규칙(스펙 §7)과 같은 결.

## 5. 실데이터 기대치 (2026-07-31 기준)

지금 사람별 작업 윤곽. 마커가 붙기 시작하면 이게 사실로 확인되거나 뒤집힐 것:

| 사람 | 문서 | 주제어로 추정한 일 |
|---|---|---|
| 돌쇠 | 87 | Agent Meter / a-mate 운영 (설치·대시보드·버전·프롬프트) |
| kimmy | 36 | a-lens + RAG 인덱싱·커넥터/MCP (두 갈래 섞임) |
| palendy | 19 | MCP playwright 대형 결과·컨텍스트 오버플로우 |
| 준냥헐 | 8 | 뚜렷한 주제 없음(건마다 다름) |
| 소금맛 | 2 | 판단 불가(도구 오류 로그 2건) |

사용자 검증 대기 중이던 항목: kimmy가 두 프로젝트를 겸하는지, 준냥헐의 발행이 일부만
되는 건지, 소금맛의 a-mate 푸시가 꺼져 있는지.

## 6. 참고

- 협업 지도 스펙: [../../../../design/a-lens/specs/2026-07-31-collab-graph.md](../../../../design/a-lens/specs/2026-07-31-collab-graph.md)
- a-mate 제약: [../../../../../a-mate/CLAUDE.md](../../../../../a-mate/CLAUDE.md)
- a-lens 제약(협업 지도 우회 포함): [../../../../../a-lens/CLAUDE.md](../../../../../a-lens/CLAUDE.md)
