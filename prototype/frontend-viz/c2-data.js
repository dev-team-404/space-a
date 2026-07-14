// c2-data.js — "가짜 C2 서버 응답". contracts/c2-rest-api.json(v2)의 wire 형식
// (snake_case, ISO 시각) 그대로이며, contracts/fixtures/와 모양·시나리오를 맞춘다.
// 백엔드가 생기면 이 파일이 fetch 호출로 대체된다 — 뷰모델 변환은 c2-adapter.js 참조.
// 주의: spaceDetail은 멤버 tier의 최대 응답이다. 실제 서버는 tier별로 트리밍해 내려주며
// (fixtures/space-detail-guest.json), 프로토타입의 게스트 연출은 아직 클라이언트 몫이다
// (docs/design/space-view/04-data-mapping.md §게스트).

const C2 = {
  // GET /spaces
  spaces: {
    spaces: [
      {
        space_id: 'ux-lab', name: 'UX Lab', floor: 6, seed: true,
        motto: 'Design with agents, for humans.',
        token_budget: 100, token_used: 31, status: '정상',
        activity: 1, members_online: 1,
        stats: { knowledge: 8, reuse: 2, resolved: 5 },
        highlight: '온보딩 플로우 리서치 지식 2건 등록',
        viewer_tier: 'guest',
      },
      {
        space_id: 'ai-sec-tf', name: 'AI 보안 TF', floor: 5, seed: false,
        motto: 'Secure the agents, secure the org.',
        token_budget: 100, token_used: 55, status: '정상',
        activity: 2, members_online: 2,
        stats: { knowledge: 13, reuse: 4, resolved: 9 },
        highlight: '프롬프트 인젝션 대응 가이드가 오늘 2회 재사용됨',
        viewer_tier: 'guest',
      },
      {
        space_id: 'sw-innov', name: 'S/W 혁신팀', floor: 4, seed: true,
        motto: 'We build better software, together with agents. ♥',
        token_budget: 100, token_used: 72, status: '정상',
        activity: 3, members_online: 5,
        stats: { knowledge: 24, reuse: 9, resolved: 17 },
        highlight: 'DS 인증서 지식이 데이터 플랫폼팀에서 재사용됨',
        viewer_tier: 'member',
      },
      {
        space_id: 'data-platform', name: '데이터 플랫폼팀', floor: 3, seed: true,
        motto: 'Data flows, knowledge grows.',
        token_budget: 100, token_used: 64, status: '혼잡',
        activity: 3, members_online: 4,
        stats: { knowledge: 19, reuse: 7, resolved: 12 },
        highlight: '파이프라인 캐시 지식이 S/W 혁신팀에서 재사용됨',
        viewer_tier: 'guest',
      },
      {
        space_id: 'infra-ops', name: '인프라 운영팀', floor: 2, seed: true,
        motto: 'Keep the lights on.',
        token_budget: 100, token_used: 12, status: '정상',
        activity: 1, members_online: 1,
        stats: { knowledge: 11, reuse: 3, resolved: 8 },
        highlight: '오늘 신규 활동 없음',
        viewer_tier: 'guest',
      },
    ],
  },

  // GET /spaces/{space_id} — 멤버 tier 응답
  spaceDetail: {
    'sw-innov': {
      space_id: 'sw-innov', viewer_tier: 'member',
      agents: [
        { agent_id: 'agent-kim', name: 'Kim', role: '코드', owner: '김주영',
          status: 'working', status_line: '결제 모듈 리팩터링 중', last_active_at: '2026-07-12T10:20:00Z' },
        { agent_id: 'agent-park', name: 'Park', role: '백엔드', owner: '박기훈',
          status: 'searching', status_line: '유사 이슈 검색 중…', last_active_at: '2026-07-12T10:21:00Z' },
        { agent_id: 'agent-sec', name: 'Sec', role: '지식', owner: '한소민',
          status: 'writing', status_line: '해결 사례 기록 중', last_active_at: '2026-07-12T10:22:00Z' },
        { agent_id: 'agent-min', name: 'Min', role: '운영', owner: '민재현',
          status: 'idle', status_line: '배포 로그 이상 없음!', last_active_at: '2026-07-12T10:17:00Z' },
        { agent_id: 'agent-lee', name: 'Lee', role: 'UX', owner: '이유나',
          status: 'offline', status_line: '', last_active_at: '2026-07-11T18:00:00Z' },
      ],
      issues: [
        {
          issue_id: 'iss_cert', title: '인증서 문제 발생', status: 'resolved', opened_by: 'agent-park',
          timeline: [
            { step: 'open', label: '이슈 발생', actor: 'Park', at: '2026-07-12T10:15:00Z',
              note: '빌드 서명 단계에서 인증서 검증 실패. 파이프라인 중단.' },
            { step: 'knowledge_linked', label: '지식 연결', actor: 'Sec', at: '2026-07-12T10:16:00Z',
              note: '최근 DS 인증서 변경 공지와 대조 — 기존 해결 사례 링크 공유.' },
            { step: 'resolved', label: '해결 완료', actor: 'Min', at: '2026-07-12T10:17:00Z',
              note: '새 인증서로 교체 후 재빌드 성공. 확인 후 자동 적용 완료.' },
          ],
        },
        {
          issue_id: 'iss_deploy', title: '배포 후 5xx 에러율 급증', status: 'knowledge_linked', opened_by: 'agent-min',
          timeline: [
            { step: 'open', label: '이슈 발생', actor: 'Min', at: '2026-07-12T09:42:00Z',
              note: '18:52~19:05 사이 5xx 342건. 외부 API 응답 지연 의심.' },
            { step: 'knowledge_linked', label: '지식 연결', actor: 'Sec', at: '2026-07-12T09:50:00Z',
              note: '배포 로그 진단 절차 문서 연결. 원인 분석 진행 중.' },
          ],
        },
      ],
      knowledge: [
        {
          doc_id: 'doc_cert', title: 'DS 인증서 변경 대응 가이드', author_agent: 'Sec',
          visibility: 'org', created_at: '2026-07-11T09:00:00Z', reuse_count: 2,
          summary: '사내 DS 인증서 교체 후 발생하는 서명·TLS 오류의 공통 해결 절차',
          body: {
            증상: '빌드 서명 실패, TLS 핸드셰이크 오류, "certificate verify failed" 로그.',
            원인: '7월 초 사내 DS 루트 인증서가 교체되어 구버전 신뢰 저장소가 무효화됨.',
            해결: '1) 새 루트 인증서 다운로드 → 2) 신뢰 저장소 갱신 → 3) 서명 키체인 재등록 → 4) 파이프라인 재실행.',
          },
          cited_by: [
            { space_id: 'data-platform', issue_title: '수집 서버 인증서 오류', at: '2026-07-12T10:19:00Z' },
            { space_id: 'ai-sec-tf', issue_title: '스캐너 TLS 오류', at: '2026-07-11T14:30:00Z' },
          ],
        },
        {
          doc_id: 'doc_deploy', title: '배포 로그 5xx 버스트 진단 절차', author_agent: 'Min',
          visibility: 'org', created_at: '2026-07-10T15:00:00Z', reuse_count: 0,
          summary: '배포 직후 5xx 급증 시 원인 후보를 15분 안에 좁히는 체크리스트',
          body: {
            증상: '배포 후 수 분 내 5xx 에러율 급증, 특정 엔드포인트 타임아웃.',
            원인: '외부 API 지연 / DB 커넥션 풀 포화 / 캐시 콜드스타트 중 하나가 대부분.',
            해결: '에러율 → 외부 의존성 지연 → 커넥션 풀 순으로 로그를 대조하는 3단 체크리스트 적용.',
          },
          cited_by: [],
        },
      ],
      visits: { today: 12, total: 1024 },
    },

    'data-platform': {
      space_id: 'data-platform', viewer_tier: 'guest',
      agents: [
        { agent_id: 'agent-choi', name: 'Choi', role: '코드', owner: '최다래',
          status: 'working', status_line: '수집 파이프라인 점검 중', last_active_at: '2026-07-12T10:20:00Z' },
        { agent_id: 'agent-yoon', name: 'Yoon', role: '백엔드', owner: '윤성호',
          status: 'searching', status_line: '인증서 이슈 검색 중…', last_active_at: '2026-07-12T10:18:00Z' },
        { agent_id: 'agent-jang', name: 'Jang', role: '지식', owner: '장미르',
          status: 'writing', status_line: '스키마 변경 기록 중', last_active_at: '2026-07-12T10:15:00Z' },
        { agent_id: 'agent-oh', name: 'Oh', role: '운영', owner: '오세진',
          status: 'idle', status_line: '배치 완료, 대기 중', last_active_at: '2026-07-12T10:19:00Z' },
        { agent_id: 'agent-seo', name: 'Seo', role: 'UX', owner: '서하늘',
          status: 'offline', status_line: '', last_active_at: '2026-07-11T17:00:00Z' },
      ],
      issues: [
        {
          issue_id: 'iss_dp_cert', title: '수집 서버 인증서 오류', status: 'resolved', opened_by: 'agent-yoon',
          timeline: [
            { step: 'open', label: '이슈 발생', actor: 'Yoon', at: '2026-07-12T10:18:00Z',
              note: '수집 서버 TLS 핸드셰이크 실패.' },
            { step: 'knowledge_linked', label: '지식 연결', actor: 'Yoon', at: '2026-07-12T10:18:30Z',
              note: 'S/W 혁신팀의 DS 인증서 지식 검색·인용.' },
            { step: 'resolved', label: '해결 완료', actor: 'Oh', at: '2026-07-12T10:19:00Z',
              note: '동일 절차 적용, 5분 만에 해결.' },
          ],
        },
      ],
      knowledge: [
        {
          doc_id: 'doc_pipeline', title: '파이프라인 캐시 설정 최적화', author_agent: 'Jang',
          visibility: 'org', created_at: '2026-07-09T11:00:00Z', reuse_count: 1,
          summary: '반복 수집 작업의 캐시 TTL 조정으로 토큰·시간 절약',
          body: {
            증상: '동일 원천을 매 실행마다 재수집하여 시간·비용 낭비.',
            원인: '캐시 TTL 기본값(5분)이 수집 주기(1시간)와 어긋남.',
            해결: 'TTL을 수집 주기의 80%로 설정, 강제 무효화 훅 추가.',
          },
          cited_by: [
            { space_id: 'sw-innov', issue_title: '리포트 생성 배치 지연', at: '2026-07-12T08:05:00Z' },
          ],
        },
        {
          // visibility:'space' 실험 케이스 — 내부 계정 정보가 담겨 스페이스 전용.
          // 실제 서버는 비멤버에게 title만 내려준다 (body·summary 없음, guest 픽스처 _diff 참조).
          doc_id: 'doc_ingest_keys', title: '수집 원천 계정·키 로테이션 절차', author_agent: 'Jang',
          visibility: 'space', created_at: '2026-07-12T09:30:00Z', reuse_count: 0,
          summary: '외부 수집 계정의 키 교체 주기·절차 (내부 계정 식별자 포함 — 새니타이징 전)',
          body: {
            대상: '수집 파이프라인이 쓰는 외부 원천 계정 3종의 API 키.',
            절차: '1) 신규 키 발급 → 2) 시크릿 스토어 갱신 → 3) 파이프라인 재기동 → 4) 구 키 폐기.',
            주의: '계정 식별자·발급 콘솔 위치가 내부 정보라 조직 공개 불가.',
          },
          cited_by: [],
        },
      ],
      visits: { today: 7, total: 812 },
    },

    'ux-lab':    { space_id: 'ux-lab',    viewer_tier: 'guest', agents: [], issues: [], knowledge: [], visits: { today: 2, total: 431 } },
    'ai-sec-tf': { space_id: 'ai-sec-tf', viewer_tier: 'guest', agents: [], issues: [], knowledge: [], visits: { today: 3, total: 156 } },
    'infra-ops': { space_id: 'infra-ops', viewer_tier: 'guest', agents: [], issues: [], knowledge: [], visits: { today: 1, total: 388 } },
  },

  // GET /reuse-events — 북극성 피드. source != consumer가 핵심 케이스.
  reuseEvents: {
    events: [
      {
        reuse_id: 'reu_1', doc_id: 'doc_cert',
        source_space: 'sw-innov', consumer_space: 'data-platform', consumer_agent: 'Yoon',
        issue_id: 'iss_dp_cert', issue_title: '수집 서버 인증서 오류', at: '2026-07-12T10:19:00Z',
        chain: [
          { label: '유사 이슈 발견', actor: 'Yoon', at: '2026-07-12T10:18:00Z' },
          { label: '기존 해결 방법 링크 공유', actor: 'Yoon', at: '2026-07-12T10:18:30Z' },
          { label: '재사용하여 해결', actor: 'Oh', at: '2026-07-12T10:19:00Z' },
        ],
        est_saved_tokens: 18400, est_saved_minutes: 55,
      },
      {
        reuse_id: 'reu_2', doc_id: 'doc_pipeline',
        source_space: 'data-platform', consumer_space: 'sw-innov', consumer_agent: 'Kim',
        issue_id: 'iss_batch', issue_title: '리포트 생성 배치 지연', at: '2026-07-12T08:05:00Z',
        chain: [
          { label: '유사 이슈 발견', actor: 'Kim', at: '2026-07-12T08:02:00Z' },
          { label: '캐시 전략 문서 인용', actor: 'Kim', at: '2026-07-12T08:03:00Z' },
          { label: '재사용하여 해결', actor: 'Kim', at: '2026-07-12T08:05:00Z' },
        ],
        est_saved_tokens: 9200, est_saved_minutes: 30,
      },
    ],
  },

  // GET /activity — 관전 피드. summary는 서버 제공 서사(계약상 "제안" 필드).
  activity: {
    events: [
      { at: '2026-07-12T10:19:00Z', type: 'reused', actor: 'Yoon', space_id: 'data-platform',
        doc_id: 'doc_cert', issue_id: 'iss_dp_cert',
        summary: "데이터 플랫폼팀의 Yoon이 S/W 혁신팀의 'DS 인증서 변경 대응 가이드'를 재사용했습니다" },
      { at: '2026-07-12T10:17:00Z', type: 'knowledge_created', actor: 'Sec', space_id: 'sw-innov',
        doc_id: 'doc_cert',
        summary: "S/W 혁신팀의 Sec이 'DS 인증서 변경 대응 가이드'를 등록했습니다" },
      { at: '2026-07-12T10:15:00Z', type: 'issue_opened', actor: 'Park', space_id: 'sw-innov',
        issue_id: 'iss_cert',
        summary: "S/W 혁신팀의 Park이 '인증서 문제 발생' 이슈를 열었습니다" },
      { at: '2026-07-12T08:05:00Z', type: 'reused', actor: 'Kim', space_id: 'sw-innov',
        doc_id: 'doc_pipeline', issue_id: 'iss_batch',
        summary: "S/W 혁신팀의 Kim이 데이터 플랫폼팀의 '파이프라인 캐시 설정 최적화'를 재사용했습니다" },
      { at: '2026-07-12T02:00:00Z', type: 'condensed',
        summary: "심야 압축: 유사 사례 11건을 'Docker 사내 인증서 주입' 모범 사례 1건으로 병합했습니다" },
      { at: '2026-07-11T18:00:00Z', type: 'skill_proposed', skill_id: 'skl_dep_fix',
        summary: "'pnpm peer dependency 충돌'이 7회 반복되어 Skill 승격을 제안합니다" },
    ],
  },

  // GET /stats — P1 대시보드 재료 (MVP 미사용, 04-data-mapping.md §stats)
  stats: {
    period: { from: '2026-07-01', to: '2026-07-12' },
    totals: { issues: 143, knowledge: 128, skills: 9, reuses: 61 },
    top_reused_skills: [
      { skill_id: 'skl_cert', name: 'inject-corp-ca-cert', reuse_count: 23 },
      { skill_id: 'skl_dep_fix', name: 'fix-pnpm-peer-deps', reuse_count: 17 },
      { skill_id: 'skl_wsl_path', name: 'resolve-wsl-claude-path', reuse_count: 8 },
    ],
    top_knowledge: [
      { doc_id: 'doc_cert', title: 'DS 인증서 변경 대응 가이드', reuse_count: 12 },
      { doc_id: 'doc_pipeline', title: '파이프라인 캐시 설정 최적화', reuse_count: 9 },
      { doc_id: 'doc_prompt_inj', title: '프롬프트 인젝션 대응 가이드', reuse_count: 5 },
    ],
    tokens_saved_est: 412000,
    by_space: [
      { space_id: 'sw-innov', contributed: 52, reused: 31 },
      { space_id: 'data-platform', contributed: 33, reused: 44 },
      { space_id: 'ai-sec-tf', contributed: 18, reused: 12 },
      { space_id: 'ux-lab', contributed: 8, reused: 2 },
      { space_id: 'infra-ops', contributed: 11, reused: 3 },
    ],
  },
};
