// client-data.js — C2 계약 밖에서 오는 데이터 (docs/design/space-view/04-data-mapping.md §갭).
// G1 인증 세션(currentUser), G2 매니저 코너(관리 API로 재편 — 재설계 대기), G5 레이아웃 상수.

const CLIENT = {
  // G1: 실제로는 로그인 세션에서 온다. 스페이스별 멤버십은 C2의 viewer_tier로 유도.
  currentUser: { id: 'kimmy', name: '김주영', agentId: 'agent-kim' },

  // G2: 매니저 코너의 실체는 #9에서 "관리 API(C4)"로 재편됐다. 씬의 매니저 캐릭터·코너를
  // 뭘로 채울지 재설계 전까지의 자리표시 데이터 — C2 wire 데이터에 넣지 않는다.
  managerAgents: [
    { id: 'manager-a', name: 'Manager_A', role: 'manager', spaceId: 'sw-innov', owner: '(팀 공용)',
      status: 'working', statusLine: '권한 및 자원 최적화 제안', deskSlot: -1 },
    { id: 'manager-d', name: 'Manager_D', role: 'manager', spaceId: 'data-platform', owner: '(팀 공용)',
      status: 'working', statusLine: '토큰 예산 재배분 중', deskSlot: -1 },
  ],
  managerEvents: [
    { spaceId: 'sw-innov', kind: 'optimize', summary: '미사용 MCP 2건 정리 제안', ts: '09:00' },
    { spaceId: 'sw-innov', kind: 'permission', summary: 'RBAC 적용 중 — 신규 에이전트 1건 승인 대기', ts: '08:40' },
    { spaceId: 'data-platform', kind: 'token', summary: '토큰 예산 80% 도달 예상 — 재배분 검토', ts: '10:02' },
  ],
};

// G5: 레이아웃은 클라이언트 소유 — 로비 사옥 이미지의 층 히트존 좌표
// (% 기준, assets/lobby-building.png 실측 보정값)
const FLOOR_ZONES = [
  { floor: 6, top: 10.0, height: 11.0 },
  { floor: 5, top: 21.0, height: 11.5 },
  { floor: 4, top: 32.5, height: 11.5 },
  { floor: 3, top: 44.0, height: 11.5 },
  { floor: 2, top: 55.5, height: 11.5 },
  { floor: 1, top: 67.0, height: 11.5 }, // 빈 층 — 새 스페이스 만들기 자리
];
const GROUND_ZONE = { top: 78.5, height: 15.0 }; // 1층 로비(게시판)
