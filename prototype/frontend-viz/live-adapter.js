// live-adapter.js — live-data.js(실제 GitHub PR 스냅숏)를 mock DB에 병합해
// "SPACE-A 개발팀" 스페이스를 1층(빈 층)에 만든다. 스냅숏이 없으면 조용히 건너뛴다.
// 매핑: PR 작성자 → 에이전트(열린 PR = "작업 중" 말풍선), PR → 이슈 타임라인(생성→머지).
(function () {
  if (typeof LIVE_PRS === 'undefined' || !Array.isArray(LIVE_PRS) || !LIVE_PRS.length) return;

  const SPACE_ID = 'space-a-dev';
  const CURRENT_USER_LOGIN = 'JuyoungKimmy-Kim';
  const TEAM = {
    'JuyoungKimmy-Kim': { agent: 'Juyoung', owner: '김주영', role: 'ux' },
    'msaltnet':         { agent: 'Msalt',   owner: '정성문', role: 'backend' },
    'palendy':          { agent: 'Palendy', owner: 'palendy', role: 'knowledge' },
  };
  const infoOf = (login) =>
    TEAM[login] || { agent: login.slice(0, 8), owner: login, role: 'code' };

  const fmt = (iso) => {
    const d = new Date(iso);
    const p = (n) => String(n).padStart(2, '0');
    return `${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`;
  };
  const short = (t, n = 36) => (t.length > n ? t.slice(0, n) + '…' : t);
  // 서사 번역 캐시 (update-live.mjs가 생성) — 없으면 원문 제목으로 폴백
  const NAR = typeof LIVE_NARRATIVES !== 'undefined' ? LIVE_NARRATIVES : {};
  const workOf = (p) => (NAR[p.number] && NAR[p.number].label) || short(p.title, 14);
  const titleOf = (p) => (NAR[p.number] && NAR[p.number].title) || short(p.title);
  const summaryOf = (p) => (NAR[p.number] && NAR[p.number].summary) || '';

  const prs = LIVE_PRS.slice().sort((a, b) => new Date(b.updatedAt) - new Date(a.updatedAt));
  const open = prs.filter((p) => p.state === 'OPEN');
  const merged = prs.filter((p) => p.state === 'MERGED');
  const latest = prs[0];

  // ── 스페이스 (1층 빈 층에 입주) ──
  DB.spaces.push({
    id: SPACE_ID, name: 'SPACE-A 개발팀', floor: 1, seed: true,
    motto: 'Building the space, inside the space.',
    tokenBudget: 100, tokenUsed: Math.min(95, prs.length * 7),
    status: open.length >= 3 ? '혼잡' : '정상',
    stats: { knowledge: merged.length, reuse: 0, resolved: merged.length },
    highlight: `${titleOf(latest)} ${latest.state === 'MERGED' ? '완료' : '진행 중'}`,
    activity: Math.min(3, open.length + 1),
    membersOnline: 0, // 아래에서 에이전트 상태로 채움
  });

  // ── PR 작성자 → 에이전트 (책상 5개 한도) ──
  const authors = [...new Set(prs.map((p) => p.author.login))].slice(0, 5);
  const liveAgents = authors.map((login, i) => {
    const info = infoOf(login);
    const myOpen = open.find((p) => p.author.login === login);
    const myMerged = merged.find((p) => p.author.login === login);
    return {
      id: 'agent-live-' + login.toLowerCase(), name: info.agent, role: info.role,
      spaceId: SPACE_ID, owner: info.owner, deskSlot: i,
      status: myOpen ? 'working' : myMerged ? 'idle' : 'offline',
      statusLine: myOpen ? `${workOf(myOpen)} 중`
        : myMerged ? `${workOf(myMerged)} 완료` : '',
    };
  });
  DB.agents.push(...liveAgents, {
    id: 'manager-live', name: 'Manager_Dev', role: 'manager', spaceId: SPACE_ID,
    owner: '(팀 공용)', deskSlot: -1,
    status: open.length ? 'working' : 'idle',
    statusLine: open.length ? `PR 리뷰 대기 ${open.length}건` : '리뷰 대기 없음',
  });
  DB.spaces.find((s) => s.id === SPACE_ID).membersOnline =
    liveAgents.filter((a) => a.status !== 'offline').length;

  // ── PR → 이슈 타임라인 (생성 → 머지) ──
  DB.issues.push(...prs.map((p) => ({
    id: 'iss-pr-' + p.number, spaceId: SPACE_ID,
    title: titleOf(p),
    status: p.state === 'MERGED' ? 'resolved' : 'open',
    timeline: [
      { step: 'open', label: '작업 시작', actor: infoOf(p.author.login).agent,
        ts: fmt(p.createdAt), note: summaryOf(p) || `리뷰 요청 (PR #${p.number})` },
      ...(p.state === 'MERGED'
        ? [{ step: 'resolved', label: '반영 완료', actor: infoOf(p.author.login).agent,
             ts: fmt(p.mergedAt || p.updatedAt), note: `리뷰를 거쳐 반영 완료 (PR #${p.number}).` }]
        : []),
    ],
  })));

  // ── 머지된 PR → 지식 문서 (완료된 작업 기록 = org 공개 자산) ──
  DB.knowledge.push(...merged.slice(0, 3).map((p) => ({
    id: 'k-pr-' + p.number, spaceId: SPACE_ID,
    title: titleOf(p), author: infoOf(p.author.login).agent,
    visibility: 'org', ts: fmt(p.mergedAt || p.updatedAt).slice(0, 5),
    summary: summaryOf(p) || `PR #${p.number}로 반영된 작업 기록`,
    body: {
      무엇: summaryOf(p) || short(p.title, 80),
      원문: `PR #${p.number} — ${short(p.title, 60)}`,
      상태: '리뷰를 거쳐 반영 완료',
    },
    citedBy: [],
  })));

  // ── PR 이벤트 → activity (하이라이트 관문·로비 게시판의 선정 풀) ──
  // 재사용(reused)은 합성하지 않는다 — PR에는 "타팀 지식 인용" 개념이 없어 북극성 지표를
  // 가짜로 채우게 된다. 머지 = knowledge_created(org-safe), 진행 = issue_opened(멤버 전용).
  DB.activity.push(
    ...merged.slice(0, 2).map((p) => ({
      ts: fmt(p.mergedAt || p.updatedAt).slice(-5), at: p.mergedAt || p.updatedAt,
      type: 'knowledge_created', spaceId: SPACE_ID, docId: 'k-pr-' + p.number,
      summary: `SPACE-A 개발팀의 ${infoOf(p.author.login).agent}이 '${titleOf(p)}'를 반영 완료했습니다`,
    })),
    ...open.slice(0, 2).map((p) => ({
      ts: fmt(p.updatedAt).slice(-5), at: p.updatedAt,
      type: 'issue_opened', spaceId: SPACE_ID,
      summary: `SPACE-A 개발팀의 ${infoOf(p.author.login).agent}이 '${titleOf(p)}' 작업을 시작했습니다`,
    })),
  );

  DB.visits[SPACE_ID] = { today: open.length + 1, total: prs.length * 3 };

  // 내 에이전트가 이 방 멤버면 나도 멤버 (멤버십 파생 규칙 그대로)
  const me = liveAgents.find((a) => a.id === 'agent-live-' + CURRENT_USER_LOGIN.toLowerCase());
  if (me) DB.memberships.push({ spaceId: SPACE_ID, agentId: me.id, owner: DB.currentUser.id });
})();
