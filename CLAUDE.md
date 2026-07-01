# Agent Mentor — 프로젝트 제약

- 제품명: Agent Mentor. 식별자 `agent-mentor`. Claude 외 타 에이전트 확장을 염두에 둔 이름이므로,
  에이전트별 로직은 하드코딩하지 말고 SourceAdapter / Engine 인터페이스 뒤로 추상화할 것.
- 최종 스택: Tauri v2 + Rust 백엔드. v1 API(SystemTray, tauri::updater, WindowBuilder 등) 금지.
  단, 현재는 Tauri 셸 이전의 순수 백엔드 크레이트 단계다. 공개 함수는 나중에 #[tauri::command]로
  배선 가능하게 유지한다.
- 상주/업데이트/자동시작은 (도입 시) 반드시 v2 공식 플러그인(tray-icon, updater, autostart)으로.
- 플랫폼: Windows 전용. macOS/Linux 분기 불필요.
- 프라이버시: 트랜스크립트는 기본 로컬 처리. 외부 전송은 Engine 선택(사내 on-prem 기본)으로만.
- 무거운 데이터 처리(JSONL 파싱/집계/감시)는 Rust 백엔드에서.
- 설계 스펙: docs/specs/, 구현 계획: docs/plans/.
