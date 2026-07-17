// c2-adapter.js — C2 wire 응답(c2-data.js) → 화면 뷰모델(DB) 번역.
// 필드 단위 매핑의 스펙은 docs/design/a-lens/04-data-mapping.md.
// 백엔드 연동 시 c2-data.js가 fetch로 바뀌어도 이 변환 계층은 그대로 남는다.

const DB = (() => {
  const pad = (n) => String(n).padStart(2, '0');
  const fmtTime = (iso) => {
    const d = new Date(iso);
    return `${pad(d.getHours())}:${pad(d.getMinutes())}`;
  };
  const fmtDate = (iso) => {
    const d = new Date(iso);
    return `${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
  };

  // C2의 role은 자유 문자열(서버 표시용) — 로봇 색/라벨용 클라이언트 키로 매핑
  const ROLE_KEY = {
    '코드': 'code', '백엔드': 'backend', '지식': 'knowledge', 'UX': 'ux', '운영': 'ops',
    '빌드/배포': 'code', '보안': 'knowledge', '매니저': 'manager',
  };

  const db = {
    currentUser: CLIENT.currentUser,
    spaces: [], memberships: [], agents: [], issues: [], knowledge: [],
    reuseEvents: [], activity: [], visits: {},
    managerEvents: CLIENT.managerEvents.slice(),
  };

  // GET /spaces → 로비 (공간·집계 + 서버 제공 하이라이트 서사)
  for (const s of C2.spaces.spaces) {
    db.spaces.push({
      id: s.space_id, name: s.name, floor: s.floor, seed: s.seed, motto: s.motto,
      tokenBudget: s.token_budget, tokenUsed: s.token_used, status: s.status,
      stats: s.stats, highlight: s.highlight,
      activity: s.activity, membersOnline: s.members_online,
    });
    // 멤버십은 viewer_tier에서 유도 (G1) — "내 에이전트가 멤버면 나도 멤버"의 서버측 판정
    if (s.viewer_tier === 'member' || s.viewer_tier === 'manager') {
      db.memberships.push({ spaceId: s.space_id, agentId: CLIENT.currentUser.agentId, owner: CLIENT.currentUser.id });
    }
  }

  // GET /spaces/{id} → 씬(에이전트)·피드(이슈)·모달(지식 원문)·방문 수
  for (const [spaceId, detail] of Object.entries(C2.spaceDetail)) {
    detail.agents.forEach((a, i) => {
      db.agents.push({
        id: a.agent_id, name: a.name, role: ROLE_KEY[a.role] || 'code',
        spaceId, owner: a.owner, status: a.status, statusLine: a.status_line,
        deskSlot: i, // 좌표는 클라이언트 소유 (G5) — 나열 순서대로 책상 배정
      });
    });
    for (const iss of detail.issues) {
      db.issues.push({
        id: iss.issue_id, spaceId, title: iss.title, status: iss.status,
        timeline: iss.timeline.map((t) => ({
          step: t.step, label: t.label, actor: t.actor, ts: fmtTime(t.at), note: t.note,
        })),
      });
    }
    for (const k of detail.knowledge) {
      db.knowledge.push({
        id: k.doc_id, spaceId, title: k.title, author: k.author_agent,
        visibility: k.visibility, ts: fmtDate(k.created_at), // created_at은 계약 추가 요청 후보 (G4)
        summary: k.summary, body: k.body,
        citedBy: k.cited_by.map((c) => ({
          spaceId: c.space_id, issueTitle: c.issue_title, ts: fmtTime(c.at),
        })),
      });
    }
    db.visits[spaceId] = detail.visits;
  }

  // 매니저 캐릭터는 C2 밖 (G2) — 재설계 전까지 클라이언트 데이터로 씬을 채운다
  db.agents.push(...CLIENT.managerAgents);

  // GET /reuse-events → 재사용 체인 피드 (북극성). est_saved_*는 '~' 라벨 필수.
  for (const r of C2.reuseEvents.events) {
    db.reuseEvents.push({
      id: r.reuse_id, knowledgeId: r.doc_id,
      sourceSpace: r.source_space, consumerSpace: r.consumer_space, consumerAgent: r.consumer_agent,
      issueTitle: r.issue_title,
      chain: r.chain.map((c) => ({ label: c.label, actor: c.actor, ts: fmtTime(c.at) })),
      estSavedTokens: r.est_saved_tokens, estSavedMinutes: r.est_saved_minutes,
    });
  }

  // GET /activity → 로비 게시판·하이라이트 관문 (summary 서사는 그대로, 선정은 구조 필드로)
  db.activity = C2.activity.events.map((e) => ({
    ts: fmtTime(e.at), at: e.at, type: e.type, summary: e.summary,
    spaceId: e.space_id, docId: e.doc_id,
  }));

  // GET /stats → 팀 리더용 대시보드 (전부 집계 번역 — LLM 불필요)
  db.stats = {
    period: C2.stats.period,
    totals: C2.stats.totals,
    tokensSavedEst: C2.stats.tokens_saved_est, // 추정치 — 렌더 시 '~' 라벨 필수
    topReusedSkills: C2.stats.top_reused_skills.map((s) => ({ name: s.name, reuseCount: s.reuse_count })),
    topKnowledge: C2.stats.top_knowledge.map((k) => ({ id: k.doc_id, title: k.title, reuseCount: k.reuse_count })),
    bySpace: C2.stats.by_space.map((b) => ({ spaceId: b.space_id, contributed: b.contributed, reused: b.reused })),
  };

  return db;
})();
