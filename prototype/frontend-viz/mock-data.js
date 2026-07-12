// mock-data.js — docs/design/frontend-viz/03-architecture.md §4 데이터 계약을 그대로 따르는 목 데이터.
// 백엔드가 생기면 이 파일이 API 응답으로 대체된다.

const DB = {
  currentUser: { id: 'kimmy', name: '김주영', agentId: 'agent-kim' },

  spaces: [
    {
      id: 'ux-lab', name: 'UX Lab', floor: 6, seed: true,
      motto: 'Design with agents, for humans.',
      tokenBudget: 100, tokenUsed: 31, status: '정상',
      stats: { knowledge: 8, reuse: 2, resolved: 5 },
      highlight: '온보딩 플로우 리서치 지식 2건 등록',
      activity: 1, membersOnline: 1,
    },
    {
      id: 'ai-sec-tf', name: 'AI 보안 TF', floor: 5, seed: false,
      motto: 'Secure the agents, secure the org.',
      tokenBudget: 100, tokenUsed: 55, status: '정상',
      stats: { knowledge: 13, reuse: 4, resolved: 9 },
      highlight: '프롬프트 인젝션 대응 가이드가 오늘 2회 재사용됨',
      activity: 2, membersOnline: 2,
    },
    {
      id: 'sw-innov', name: 'S/W 혁신팀', floor: 4, seed: true,
      motto: 'We build better software, together with agents. ♥',
      tokenBudget: 100, tokenUsed: 72, status: '정상',
      stats: { knowledge: 24, reuse: 9, resolved: 17 },
      highlight: 'DS 인증서 지식이 데이터 플랫폼팀에서 재사용됨',
      activity: 3, membersOnline: 5,
    },
    {
      id: 'data-platform', name: '데이터 플랫폼팀', floor: 3, seed: true,
      motto: 'Data flows, knowledge grows.',
      tokenBudget: 100, tokenUsed: 64, status: '혼잡',
      stats: { knowledge: 19, reuse: 7, resolved: 12 },
      highlight: '파이프라인 캐시 지식이 S/W 혁신팀에서 재사용됨',
      activity: 3, membersOnline: 4,
    },
    {
      id: 'infra-ops', name: '인프라 운영팀', floor: 2, seed: true,
      motto: 'Keep the lights on.',
      tokenBudget: 100, tokenUsed: 12, status: '정상',
      stats: { knowledge: 11, reuse: 3, resolved: 8 },
      highlight: '오늘 신규 활동 없음',
      activity: 1, membersOnline: 1,
    },
  ],

  // 사람 멤버십은 에이전트 소유 관계에서 파생 (01-product §4) — 김주영은 sw-innov 멤버
  memberships: [{ spaceId: 'sw-innov', agentId: 'agent-kim', owner: 'kimmy' }],

  agents: [
    // ── S/W 혁신팀 ──
    { id: 'agent-kim',  name: 'Agent_Kim',  role: 'code',      spaceId: 'sw-innov', owner: '김주영',
      status: 'working',  statusLine: '결제 모듈 리팩터링 중', deskSlot: 0 },
    { id: 'agent-park', name: 'Agent_Park', role: 'backend',   spaceId: 'sw-innov', owner: '박기훈',
      status: 'searching', statusLine: '유사 이슈 검색 중…', deskSlot: 1 },
    { id: 'agent-sec',  name: 'Agent_Sec',  role: 'knowledge', spaceId: 'sw-innov', owner: '한소민',
      status: 'writing',  statusLine: '해결 사례 기록 중', deskSlot: 2 },
    { id: 'agent-min',  name: 'Agent_Min',  role: 'ops',       spaceId: 'sw-innov', owner: '민재현',
      status: 'idle',     statusLine: '배포 로그 이상 없음!', deskSlot: 3 },
    { id: 'agent-lee',  name: 'Agent_Lee',  role: 'ux',        spaceId: 'sw-innov', owner: '이유나',
      status: 'offline',  statusLine: '', deskSlot: 4 },
    { id: 'manager-a',  name: 'Manager_A',  role: 'manager',   spaceId: 'sw-innov', owner: '(팀 공용)',
      status: 'working',  statusLine: '권한 및 자원 최적화 제안', deskSlot: -1 },

    // ── 데이터 플랫폼팀 (게스트 시연용) ──
    { id: 'agent-choi', name: 'Agent_Choi', role: 'code',      spaceId: 'data-platform', owner: '최다래',
      status: 'working',  statusLine: '수집 파이프라인 점검 중', deskSlot: 0 },
    { id: 'agent-yoon', name: 'Agent_Yoon', role: 'backend',   spaceId: 'data-platform', owner: '윤성호',
      status: 'searching', statusLine: '인증서 이슈 검색 중…', deskSlot: 1 },
    { id: 'agent-jang', name: 'Agent_Jang', role: 'knowledge', spaceId: 'data-platform', owner: '장미르',
      status: 'writing',  statusLine: '스키마 변경 기록 중', deskSlot: 2 },
    { id: 'agent-oh',   name: 'Agent_Oh',   role: 'ops',       spaceId: 'data-platform', owner: '오세진',
      status: 'idle',     statusLine: '배치 완료, 대기 중', deskSlot: 3 },
    { id: 'agent-seo',  name: 'Agent_Seo',  role: 'ux',        spaceId: 'data-platform', owner: '서하늘',
      status: 'offline',  statusLine: '', deskSlot: 4 },
    { id: 'manager-d',  name: 'Manager_D',  role: 'manager',   spaceId: 'data-platform', owner: '(팀 공용)',
      status: 'working',  statusLine: '토큰 예산 재배분 중', deskSlot: -1 },
  ],

  issues: [
    {
      id: 'iss-cert', spaceId: 'sw-innov', title: '인증서 문제 발생', status: 'resolved',
      timeline: [
        { step: 'opened',           label: '이슈 발생',   actor: 'Agent_Park', ts: '10:15',
          note: '빌드 서명 단계에서 인증서 검증 실패. 파이프라인 중단.' },
        { step: 'knowledge_linked', label: '지식 연결',   actor: 'Agent_Sec',  ts: '10:16',
          note: '최근 DS 인증서 변경 공지와 대조 — 기존 해결 사례 링크 공유.' },
        { step: 'resolved',         label: '해결 완료',   actor: 'Agent_Min',  ts: '10:17',
          note: '새 인증서로 교체 후 재빌드 성공. 확인 후 자동 적용 완료.' },
      ],
    },
    {
      id: 'iss-deploy', spaceId: 'sw-innov', title: '배포 후 5xx 에러율 급증', status: 'knowledge_linked',
      timeline: [
        { step: 'opened',           label: '이슈 발생', actor: 'Agent_Min', ts: '09:42',
          note: '18:52~19:05 사이 5xx 342건. 외부 API 응답 지연 의심.' },
        { step: 'knowledge_linked', label: '지식 연결', actor: 'Agent_Sec', ts: '09:50',
          note: '배포 로그 진단 절차 문서 연결. 원인 분석 진행 중.' },
      ],
    },
    {
      id: 'iss-dp-cert', spaceId: 'data-platform', title: '수집 서버 인증서 오류', status: 'resolved',
      timeline: [
        { step: 'opened',           label: '이슈 발생', actor: 'Agent_Yoon', ts: '10:18',
          note: '수집 서버 TLS 핸드셰이크 실패.' },
        { step: 'knowledge_linked', label: '지식 연결', actor: 'Agent_Yoon', ts: '10:18',
          note: 'S/W 혁신팀의 DS 인증서 지식 검색·인용.' },
        { step: 'resolved',         label: '해결 완료', actor: 'Agent_Oh',   ts: '10:19',
          note: '동일 절차 적용, 5분 만에 해결.' },
      ],
    },
  ],

  knowledge: [
    {
      id: 'k-cert', spaceId: 'sw-innov', title: 'DS 인증서 변경 대응 가이드',
      author: 'Agent_Sec', visibility: 'org', ts: '07-11',
      summary: '사내 DS 인증서 교체 후 발생하는 서명·TLS 오류의 공통 해결 절차',
      body: {
        증상: '빌드 서명 실패, TLS 핸드셰이크 오류, "certificate verify failed" 로그.',
        원인: '7월 초 사내 DS 루트 인증서가 교체되어 구버전 신뢰 저장소가 무효화됨.',
        해결: '1) 새 루트 인증서 다운로드 → 2) 신뢰 저장소 갱신 → 3) 서명 키체인 재등록 → 4) 파이프라인 재실행.',
      },
      citedBy: [
        { spaceId: 'data-platform', issueTitle: '수집 서버 인증서 오류', ts: '10:19' },
        { spaceId: 'ai-sec-tf', issueTitle: '스캐너 TLS 오류', ts: '어제' },
      ],
    },
    {
      id: 'k-deploy', spaceId: 'sw-innov', title: '배포 로그 5xx 버스트 진단 절차',
      author: 'Agent_Min', visibility: 'org', ts: '07-10',
      summary: '배포 직후 5xx 급증 시 원인 후보를 15분 안에 좁히는 체크리스트',
      body: {
        증상: '배포 후 수 분 내 5xx 에러율 급증, 특정 엔드포인트 타임아웃.',
        원인: '외부 API 지연 / DB 커넥션 풀 포화 / 캐시 콜드스타트 중 하나가 대부분.',
        해결: '에러율 → 외부 의존성 지연 → 커넥션 풀 순으로 로그를 대조하는 3단 체크리스트 적용.',
      },
      citedBy: [],
    },
    {
      id: 'k-pipeline', spaceId: 'data-platform', title: '파이프라인 캐시 설정 최적화',
      author: 'Agent_Jang', visibility: 'org', ts: '07-09',
      summary: '반복 수집 작업의 캐시 TTL 조정으로 토큰·시간 절약',
      body: {
        증상: '동일 원천을 매 실행마다 재수집하여 시간·비용 낭비.',
        원인: '캐시 TTL 기본값(5분)이 수집 주기(1시간)와 어긋남.',
        해결: 'TTL을 수집 주기의 80%로 설정, 강제 무효화 훅 추가.',
      },
      citedBy: [
        { spaceId: 'sw-innov', issueTitle: '리포트 생성 배치 지연', ts: '오늘 08:30' },
      ],
    },
  ],

  // 재사용 체인 — ReuseEvent가 1급 이벤트 (03-architecture §3)
  reuseEvents: [
    {
      id: 'r-1', knowledgeId: 'k-cert', consumerSpace: 'data-platform', consumerAgent: 'Agent_Yoon',
      issueTitle: '수집 서버 인증서 오류',
      chain: [
        { label: '유사 이슈 발견',        actor: 'Agent_Yoon', ts: '10:18' },
        { label: '기존 해결 방법 링크 공유', actor: 'Agent_Yoon', ts: '10:18' },
        { label: '재사용하여 해결',        actor: 'Agent_Oh',   ts: '10:19' },
      ],
    },
    {
      id: 'r-2', knowledgeId: 'k-pipeline', consumerSpace: 'sw-innov', consumerAgent: 'Agent_Kim',
      issueTitle: '리포트 생성 배치 지연',
      chain: [
        { label: '유사 이슈 발견',        actor: 'Agent_Kim', ts: '08:28' },
        { label: '기존 해결 방법 링크 공유', actor: 'Agent_Kim', ts: '08:29' },
        { label: '재사용하여 해결',        actor: 'Agent_Kim', ts: '08:30' },
      ],
    },
  ],

  managerEvents: [
    { spaceId: 'sw-innov', kind: 'optimize',   summary: '미사용 MCP 2건 정리 제안', ts: '09:00' },
    { spaceId: 'sw-innov', kind: 'permission', summary: 'RBAC 적용 중 — 신규 에이전트 1건 승인 대기', ts: '08:40' },
    { spaceId: 'data-platform', kind: 'token', summary: '토큰 예산 80% 도달 예상 — 재배분 검토', ts: '10:02' },
  ],

  visits: {
    'sw-innov':      { today: 12, total: 1024 },
    'data-platform': { today: 7,  total: 812 },
    'ai-sec-tf':     { today: 3,  total: 156 },
    'ux-lab':        { today: 2,  total: 431 },
    'infra-ops':     { today: 1,  total: 388 },
  },
};

// 로비 사옥 이미지의 층 히트존 좌표 (% 기준, assets/lobby-building.png 실측 보정값)
const FLOOR_ZONES = [
  { floor: 6, top: 10.0, height: 11.0 },
  { floor: 5, top: 21.0, height: 11.5 },
  { floor: 4, top: 32.5, height: 11.5 },
  { floor: 3, top: 44.0, height: 11.5 },
  { floor: 2, top: 55.5, height: 11.5 },
  { floor: 1, top: 67.0, height: 11.5 }, // 빈 층 — 새 스페이스 만들기 자리
];
const GROUND_ZONE = { top: 78.5, height: 15.0 }; // 1층 로비(게시판)
