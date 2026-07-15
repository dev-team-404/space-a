// client-data.js — C2 계약 밖에서 오는 데이터 (docs/design/space-view/04-data-mapping.md §갭).
// G1 인증 세션(currentUser), G5 레이아웃 상수, 매니저 캐릭터(연출 전용 — G2 재설계).

const CLIENT = {
  // G1: 실제로는 로그인 세션에서 온다. 스페이스별 멤버십은 C2의 viewer_tier로 유도.
  currentUser: { id: 'kimmy', name: '김주영', agentId: 'agent-kim' },

  // G2 재설계(2026-07-15): 매니저는 백엔드 실체가 없는 안내 데스크 연출이다. 말풍선은
  // 어댑터가 C2 구조 필드(status·token_used)에서 결정론으로 채운다 — 여기엔 캐릭터 껍데기만.
  managerAgents: [
    { id: 'manager-a', name: 'Manager_A', role: 'manager', spaceId: 'sw-innov', owner: '(팀 공용)',
      status: 'working', statusLine: '', deskSlot: -1 },
    { id: 'manager-d', name: 'Manager_D', role: 'manager', spaceId: 'data-platform', owner: '(팀 공용)',
      status: 'working', statusLine: '', deskSlot: -1 },
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
