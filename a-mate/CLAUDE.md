# CLAUDE.md — a-mate (Agent Mentor)

Windows·WSL의 Claude Code 사용 기록을 **로컬에서** 분석해 코칭하는 Windows 상주형 데스크톱 앱.

## 개발 제약

- 폴더명 `a-mate`는 README의 Pillar 1 축 이름(**A-Mate**)과 맞춘 것이다.
  단, 앱의 제품명·코드 식별자는 그대로 **Agent Mentor** / `agent-mentor`(Cargo 크레이트,
  tauri productName, DB·로그 파일명 등)를 유지한다 — 폴더 이름만 축과 정렬했을 뿐,
  식별자 리네이밍은 아니다.
- Claude 외 타 에이전트 확장을 염두에 둔 제품이므로, 에이전트별 로직은 하드코딩하지 말고
  SourceAdapter / Engine 인터페이스 뒤로 추상화할 것.
- 스택: Tauri v2 + Rust 백엔드, 프론트엔드는 Svelte 5 + Vite + TypeScript.
  v1 API(SystemTray, tauri::updater, WindowBuilder 등) 금지.
- 상주/업데이트/자동시작은 반드시 v2 공식 플러그인(tray-icon, updater, autostart)으로.
- 플랫폼: **Windows 전용**. macOS/Linux 분기 불필요. 빌드·실행은 네이티브 Windows
  PowerShell/cmd에서 — **WSL 안에서 빌드/실행 금지** (WSL은 분석 대상일 뿐).
- 프라이버시: 트랜스크립트는 기본 로컬 처리. 외부 전송은 Engine 선택(사내 on-prem 기본)으로만.
- 무거운 데이터 처리(JSONL 파싱/집계/감시)는 Rust 백엔드(`crates/core`)에서.
  프론트엔드는 렌더링만 담당한다.

## 구조 (Cargo workspace)

| 위치 | 내용 |
|------|------|
| `crates/core/` | 순수 도메인 로직 (lib `agent_mentor`) — 파싱·집계·rules·다이어리·코칭. UI(Tauri) 비의존 — 단, Windows·WSL 경로/명령 처리(`hosts.rs` 등)는 포함 |
| `src-tauri/` | Tauri v2 셸 (`agent-mentor-app`) — 트레이·커맨드·파이프라인 런타임 |
| `src/` | Svelte 프론트엔드 — 미니홈피 UI·마스코트·설정·채팅 |

## 명령어 (Windows PowerShell, `a-mate/`에서)

```powershell
npm run tauri dev    # 개발 모드 실행 (핫리로드)
npm run tauri build  # 릴리스 빌드
npm test             # 프론트엔드 테스트 (Vitest)
cargo test           # Rust 테스트 (워크스페이스 전체)
```

## 참고

- 셋업·트러블슈팅: [docs/architecture/a-mate/build-and-run.md](../docs/architecture/a-mate/build-and-run.md)
- 과거 설계 스펙·구현 계획: [docs/archive/design/a-mate/](../docs/archive/design/a-mate/)
