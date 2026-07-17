// app.js — SPACE A 시각화 프로토타입 (Phase 1 MVP: F1~F7)
// 프레임워크 없이 상태 → HTML 문자열 렌더. 백엔드 연동 시 DB 접근부만 교체.

const state = {
  view: 'lobby',        // 'lobby' | 'space'
  spaceId: null,
  forceGuest: false,    // 멤버 스페이스에서 게스트 시점 시연용 토글
  sessionVisits: {},    // 이번 세션에서 올린 TODAY 카운트
};

const ROLE_LABEL = { code: '코드', backend: '백엔드', knowledge: '지식', ux: 'UX', ops: '운영', manager: '매니저' };
const STATUS_LABEL = { working: '작업 중', searching: '검색 중', writing: '기록 중', idle: '대기 중', offline: '오프라인' };
const STEP_ICON = { open: '!', knowledge_linked: '≡', resolved: '✓' }; // step 값은 C2 issueStatus enum
const ACTIVITY_CHIP = { reused: ['chip-reuse', '재사용'], knowledge_created: ['chip-new', '신착'], issue_opened: ['chip-new', '이슈'], condensed: ['chip-new', '압축'], skill_proposed: ['chip-new', 'Skill'] };

// ── 헬퍼 ──────────────────────────────────────────────────────────

const $app = () => document.getElementById('app');
const $modal = () => document.getElementById('modal-root');

const spaceById = (id) => DB.spaces.find((s) => s.id === id);
const agentsOf = (id) => DB.agents.filter((a) => a.spaceId === id);
const issuesOf = (id) => DB.issues.filter((i) => i.spaceId === id);
const knowledgeById = (id) => DB.knowledge.find((k) => k.id === id);
const isMember = (spaceId) => DB.memberships.some((m) => m.spaceId === spaceId && m.owner === DB.currentUser.id);
const roleFor = (spaceId) => (isMember(spaceId) && !state.forceGuest ? 'member' : 'guest');

function reuseEventsOf(spaceId) {
  return DB.reuseEvents.filter((r) => {
    const k = knowledgeById(r.knowledgeId);
    return r.consumerSpace === spaceId || (k && k.spaceId === spaceId);
  });
}

function visitsOf(spaceId) {
  const base = DB.visits[spaceId] || { today: 0, total: 0 };
  const bump = state.sessionVisits[spaceId] || 0;
  return { today: base.today + bump, total: base.total + bump };
}

// ── 오늘의 하이라이트 관문 (04-data-mapping §activity) ─────────────
// "오늘 가장 가치 있던 사건 1건" 선정 — 북극성(재사용된 시행착오) 기준 결정론 랭킹.
// 선정은 구조 필드(type·doc_id)로 소비자가 하고, 문장은 서버 제공 summary를 그대로 쓴다.

const HL_SCORE = { reused: 4, skill_proposed: 3, knowledge_created: 2, condensed: 1, issue_opened: 0 };

// 로비·게스트에 노출 가능한 이벤트 — issue_opened는 summary에 이슈 제목(멤버 전용 서사)이
// 담기므로 제외. 서버 트리밍이 정답이며 계약에 규칙 추가 필요 (04-data-mapping G7).
const ORG_SAFE_EVENT_TYPES = ['reused', 'knowledge_created', 'skill_proposed', 'condensed'];

function isCrossSpaceReuse(e) {
  if (e.type !== 'reused' || !e.docId) return false;
  const r = DB.reuseEvents.find((x) => x.knowledgeId === e.docId && x.consumerSpace === e.spaceId);
  return !!(r && r.sourceSpace && r.sourceSpace !== r.consumerSpace);
}

function pickHighlight(events) {
  const score = (e) => (HL_SCORE[e.type] ?? 0) + (isCrossSpaceReuse(e) ? 1 : 0);
  return events.slice().sort((a, b) => score(b) - score(a) || new Date(b.at) - new Date(a.at))[0] || null;
}

// 스페이스 관련 이벤트 = 그 방에서 일어났거나, 그 방의 지식이 다른 방에서 재사용된 것
function relatedToSpace(e, spaceId) {
  if (e.spaceId === spaceId) return true;
  if (e.type === 'reused' && e.docId) {
    const r = DB.reuseEvents.find((x) => x.knowledgeId === e.docId);
    return !!(r && r.sourceSpace === spaceId);
  }
  return false;
}

// ── 라우팅 ────────────────────────────────────────────────────────

function goLobby() {
  state.view = 'lobby';
  state.spaceId = null;
  render();
}

function enterSpace(id) {
  state.view = 'space';
  state.spaceId = id;
  state.forceGuest = false;
  state.sessionVisits[id] = (state.sessionVisits[id] || 0) + 1;
  render();
}

function toggleViewpoint() {
  state.forceGuest = !state.forceGuest;
  render();
}

function render() {
  closeModal();
  $app().innerHTML = state.view === 'lobby' ? lobbyHTML() : spaceHTML(state.spaceId);
  fitIso();
}

// 사무실 씬(1448×1086 디자인 공간)을 씬 영역 크기에 맞춰 스케일
function fitIso() {
  const stage = document.querySelector('.office-stage');
  const fit = document.querySelector('.office-fit');
  if (!stage || !fit) return;
  const s = Math.min(stage.clientWidth / 1470, stage.clientHeight / 1100, 1.05);
  fit.style.transform = `scale(${s})`;
}
window.addEventListener('resize', fitIso);

// ── 공용 조각 ─────────────────────────────────────────────────────

function robotHTML(agent, extra = '') {
  const crown = agent.role === 'manager' ? '<div class="crown"></div>' : '';
  const zzz = agent.status === 'offline' ? '<div class="zzz">z<span>z</span></div>' : '';
  return `
    <div class="robot role-${agent.role} state-${agent.status} ${extra}">
      ${crown}
      <div class="antenna"></div>
      <div class="head"><span class="eye"></span><span class="eye"></span></div>
      <div class="body"></div>
      ${zzz}
    </div>`;
}

function tokenGaugeHTML(space) {
  const pct = Math.round((space.tokenUsed / space.tokenBudget) * 100);
  return `
    <div class="gauge"><div class="gauge-fill ${pct >= 80 ? 'warn' : ''}" style="width:${pct}%"></div></div>
    <span class="gauge-num">${pct}%</span>`;
}

// ── 로비 (F1) ─────────────────────────────────────────────────────

function lobbyHTML() {
  const floorsHtml = FLOOR_ZONES.map((z) => {
    const space = DB.spaces.find((s) => s.floor === z.floor);
    const style = `top:${z.top}%;height:${z.height}%;`;
    if (!space) {
      return `
        <div class="floor-zone empty" style="${style}">
          <div class="floor-tag">${z.floor}F</div>
          <div class="floor-card">
            <h4>빈 층</h4>
            <p class="muted">새 스페이스를 만들고 에이전트를 초대할 수 있어요.</p>
            <span class="badge badge-dim">+ 스페이스 만들기 (준비 중)</span>
          </div>
        </div>`;
    }
    const member = isMember(space.id);
    return `
      <div class="floor-zone activity-${space.activity}" style="${style}" onclick="enterSpace('${space.id}')">
        <div class="floor-glow"></div>
        <div class="floor-tag">${z.floor}F · ${space.name}${member ? ' <span class="me-dot" title="내 스페이스"></span>' : ''}</div>
        <div class="floor-card">
          <h4>${space.name} <span class="badge ${member ? 'badge-member' : 'badge-guest'}">${member ? '멤버' : '게스트 관전'}</span></h4>
          <div class="card-stats">
            <span>에이전트 <b>${agentsOf(space.id).length || space.membersOnline}</b></span>
            <span>지식 <b>${space.stats.knowledge}</b></span>
            <span>재사용 <b>${space.stats.reuse}</b></span>
          </div>
          <p class="highlight">“${space.highlight}”</p>
          <p class="muted">클릭해서 입장 →</p>
        </div>
      </div>`;
  }).join('');

  // 관문: 가장 가치 있던 1건(★)을 맨 위에, 그 외 최신 1건 — 04-data-mapping.md §activity
  // 로비는 조직 공개 표면이므로 노출 풀 자체를 org-safe 이벤트로 제한
  const lobbyEvents = DB.activity.filter((e) => ORG_SAFE_EVENT_TYPES.includes(e.type));
  const hl = pickHighlight(lobbyEvents);
  const rest = lobbyEvents.filter((e) => e !== hl)
    .sort((a, b) => new Date(b.at) - new Date(a.at));
  const boardItems = [
    hl ? `<li class="board-hl"><span class="chip chip-hl">★ 오늘</span> ${hl.summary}</li>` : '',
    ...rest.slice(0, 1).map((e) => {
      const [chipCls, chipLabel] = ACTIVITY_CHIP[e.type] || ['chip-new', '소식'];
      return `<li><span class="chip ${chipCls}">${chipLabel}</span> ${e.summary}</li>`;
    }),
  ].join('');
  const groundHtml = `
    <div class="floor-zone ground" style="top:${GROUND_ZONE.top}%;height:${GROUND_ZONE.height}%;">
      <div class="floor-tag">G · 로비 게시판</div>
      <div class="floor-card">
        <h4>오늘의 조직 하이라이트</h4>
        <ul class="board-list">${boardItems}</ul>
      </div>
    </div>`;

  const elevatorHtml = DB.spaces
    .slice()
    .sort((a, b) => b.floor - a.floor)
    .map((s) => `
      <button class="ev-btn" onclick="enterSpace('${s.id}')">
        <span class="ev-floor">${s.floor}F</span>
        <span class="ev-name">${s.name}</span>
        <span class="ev-online online-${s.activity}">● ${s.membersOnline}</span>
      </button>`).join('');

  const totalKnowledge = DB.spaces.reduce((n, s) => n + s.stats.knowledge, 0);
  const totalReuse = DB.spaces.reduce((n, s) => n + s.stats.reuse, 0);

  return `
    <div class="lobby">
      <header class="topbar">
        <div class="logo" onclick="goLobby()">SPACE <span class="logo-a">A</span></div>
        <div class="topbar-title">회사 로비 — 층을 골라 들어가세요</div>
        <div class="topbar-right">
          <button class="toggle-btn" onclick="openDashboard()">📊 대시보드</button>
          <span class="user-chip">${DB.currentUser.name}</span>
        </div>
      </header>
      <div class="lobby-body">
        <aside class="panel elevator-panel">
          <h3>엘리베이터</h3>
          ${elevatorHtml}
          ${DB.spaces.some((s) => s.floor === 1) ? '' : `
          <button class="ev-btn ev-empty" disabled>
            <span class="ev-floor">1F</span><span class="ev-name">빈 층 — 새 스페이스</span>
          </button>`}
        </aside>
        <div class="building-wrap">
          <div class="building">
            <img src="../assets/lobby-building.png" alt="회사 사옥" draggable="false" />
            ${floorsHtml}
            ${groundHtml}
          </div>
        </div>
        <aside class="panel lobby-side">
          <h3>회사 현황</h3>
          <div class="org-stats">
            <div class="org-stat"><b>${DB.spaces.length}</b><span>스페이스</span></div>
            <div class="org-stat"><b>${DB.agents.length}</b><span>에이전트</span></div>
            <div class="org-stat"><b>${totalKnowledge}</b><span>공유 지식</span></div>
            <div class="org-stat"><b>${totalReuse}</b><span>재사용</span></div>
          </div>
          <h3>공개 지식 신착</h3>
          ${DB.knowledge.filter((k) => k.visibility === 'org').map((k) => `
            <button class="k-item" onclick="openKnowledge('${k.id}')">
              <span class="k-title">${k.title}</span>
              <span class="k-meta">${spaceById(k.spaceId).name} · 인용 ${k.citedBy.length}</span>
            </button>`).join('')}
          <p class="muted small">조직 공개(org) 지식만 로비에 올라와요. 스페이스 전용 문서는 제목까지 방 멤버 전용이에요.</p>
        </aside>
      </div>
    </div>`;
}

// ── 스페이스 (F2~F7) ──────────────────────────────────────────────

// 배경 이미지(../assets/office-room.png, 1448×1086) 픽셀 좌표 캘리브레이션.
// 각 책상의 의자 위치 = 로봇 스프라이트의 바닥 앵커.
const DESK_SLOTS = [
  { x: 346, y: 728 }, { x: 650, y: 738 }, { x: 963, y: 742 },
  { x: 452, y: 933 }, { x: 805, y: 952 },
];

function spaceHTML(id) {
  const space = spaceById(id);
  const role = roleFor(id);
  const guest = role === 'guest';
  const agents = agentsOf(id);
  const deskAgents = agents.filter((a) => a.deskSlot >= 0);
  const manager = agents.find((a) => a.role === 'manager');
  const visits = visitsOf(id);
  const memberOfThis = isMember(id);

  return `
    <div class="space-view">
      <header class="topbar">
        <div class="logo" onclick="goLobby()">SPACE <span class="logo-a">A</span></div>
        <div class="topbar-title">${space.name} <span class="floor-chip">${space.floor}F</span></div>
        <div class="topbar-right">
          ${memberOfThis
            ? `<button class="toggle-btn ${guest ? 'is-guest' : ''}" onclick="toggleViewpoint()">시점: ${guest ? '게스트 (시연)' : '멤버'}</button>`
            : `<span class="badge badge-guest">게스트 — 유리벽 관전</span>`}
          <span class="user-chip">${DB.currentUser.name}</span>
        </div>
      </header>

      <div class="space-body">
        <main class="scene ${guest ? 'guest' : ''}">
          ${guest ? `<div class="guest-banner">유리벽 관전 모드 — 방의 구성과 집계만 보여요. 상세 피드와 원문은 멤버 전용입니다.</div>` : ''}
          <div class="office-stage"><div class="office-fit">
            <div class="office">
              <img src="../assets/office-room.png" alt="" draggable="false" />
              <div class="sign">
                <div class="sign-title">${space.name} 방</div>
                <div class="sign-sub">✦ Agent Collaboration Space ✦</div>
              </div>
              ${chalkboardHTML(space, guest)}
              <div class="poster-neon">MOVE FAST<br>WITH<br>AGENTS</div>
              ${shelfBadgeHTML(space)}
              ${deskAgents.map((a) => deskHTML(a, guest)).join('')}
              ${manager ? managerHTML(space, manager, guest) : ''}
            </div>
          </div></div>
        </main>

        <aside class="sidebar">
          <div class="sidebar-head">
            <span class="hub-title">Agent Collaboration Hub</span>
            <span class="live-dot">● 분 단위 스냅숏</span>
          </div>
          ${spaceHighlightHTML(space, guest)}
          ${issueFeedHTML(space, guest)}
          ${reuseFeedHTML(space, guest)}
          ${activityFeedHTML(agents, guest)}
        </aside>
      </div>

      <footer class="bottombar">
        <span class="motto">${space.motto}</span>
        <span class="visits">TODAY <b>${visits.today}</b> · TOTAL <b>${visits.total}</b></span>
        <button class="ev-back" onclick="goLobby()">▼ 엘리베이터 (로비로)</button>
      </footer>
    </div>`;
}

// 방 입장 첫 시선 — 이 방과 관련된 오늘의 하이라이트 1건. 게스트에겐 이슈성 이벤트 제외.
function spaceHighlightHTML(space, guest) {
  const events = DB.activity.filter((e) =>
    relatedToSpace(e, space.id) && (!guest || ORG_SAFE_EVENT_TYPES.includes(e.type)));
  const hl = pickHighlight(events);
  if (!hl) return '';
  return `
    <div class="hl-card">
      <span class="chip chip-hl">★ 오늘의 하이라이트</span>
      <p>${hl.summary}</p>
    </div>`;
}

function chalkboardHTML(space, guest) {
  // 이슈 제목·타임라인은 내부 서사 — 게스트 응답에는 없다 (04-data-mapping §게스트)
  if (guest) {
    return `<div class="chalkboard"><div class="board-title">에이전트 게시판 (Agent Board)</div>
      <p class="board-empty">게시 내용은 멤버에게만 보여요</p></div>`;
  }
  const issue = issuesOf(space.id).find((i) => i.status === 'resolved') || issuesOf(space.id)[0];
  if (!issue) return '<div class="chalkboard"><div class="board-title">에이전트 게시판 (Agent Board)</div><p class="board-empty">아직 게시된 이슈가 없어요</p></div>';
  const cards = issue.timeline.map((t, i) => `
    ${i > 0 ? '<span class="board-arrow">→</span>' : ''}
    <div class="board-card">
      <div class="bc-label">${t.label}</div>
      <div class="bc-actor">${t.actor}</div>
    </div>`).join('');
  const log = issue.timeline.map((t) => `
        <div class="board-log-line"><b>${t.actor}</b>: ${t.note} <span class="ts">${t.ts}</span></div>`).join('');
  return `
    <div class="chalkboard">
      <div class="board-title">에이전트 게시판 (Agent Board)</div>
      <div class="board-issue-title">${issue.title}</div>
      <div class="board-cards">${cards}</div>
      <div class="board-log-wrap">${log}</div>
    </div>`;
}

// 배경 이미지 위 픽셀 좌표(x,y = 스프라이트 바닥 중앙)에 서 있는 스프라이트
function deskHTML(agent, guest) {
  const slot = DESK_SLOTS[agent.deskSlot] || DESK_SLOTS[0];
  const bubbleText = guest ? STATUS_LABEL[agent.status] : agent.statusLine;
  const showBubble = agent.status !== 'offline' && bubbleText;
  return `
    <div class="sprite" style="left:${slot.x}px;top:${slot.y}px;z-index:${Math.round(slot.y)}">
      ${showBubble ? `<div class="bubble ${guest ? 'generic' : ''}">${bubbleText}</div>` : ''}
      <div class="robot-scale">${robotHTML(agent)}</div>
      <div class="nameplate" title="${guest ? '' : '담당: ' + agent.owner}">
        ${guest ? ROLE_LABEL[agent.role] + ' 에이전트' : agent.name}
      </div>
    </div>`;
}

// 매니저: 부스(이미지에 구워진 나무 단상) 뒤에 로봇, 그 아래에 정보 패널
function managerHTML(space, manager, guest) {
  const events = DB.managerEvents.filter((e) => e.spaceId === space.id);
  return `
    <div class="sprite" style="left:1272px;top:486px;z-index:486">
      <div class="robot-scale mgr">${robotHTML(manager)}</div>
      <div class="nameplate">${guest ? '매니저 에이전트' : manager.name}</div>
    </div>
    <div class="manager-corner">
      <div class="mc-title">Manager Agent Corner</div>
      <div class="mc-rows">
        <div class="mc-row"><span>Token 사용량</span>${tokenGaugeHTML(space)}</div>
        <div class="mc-row"><span>Room 상태</span><b class="${space.status === '정상' ? 'ok' : 'busy'}">● ${space.status}</b></div>
        <div class="mc-row"><span>권한 관리</span><b>RBAC 적용 중</b></div>
      </div>
      ${events[0] ? `<div class="mc-note">“${events[0].summary}”</div>` : ''}
    </div>`;
}

// 책장(이미지에 구워짐) 앞에 지식·재사용 집계 배지
function shelfBadgeHTML(space) {
  return `
    <div class="shelf-badges">
      <span class="shelf-chip">📚 지식 ${space.stats.knowledge}</span>
      <span class="shelf-chip">🏆 재사용 ${space.stats.reuse}</span>
    </div>`;
}

// ── 사이드바 피드 (F4·F5 + F7 가시성) ────────────────────────────

function issueFeedHTML(space, guest) {
  // 게스트에게 이슈는 제목까지 비노출 — C2가 issues: []를 내려준다 (04-data-mapping §게스트)
  if (guest) {
    return feedSection('이슈 흐름', 'Issue Flow',
      `<div class="feed-card locked-card"><div class="lock-note">이슈 흐름은 멤버 전용이에요</div></div>`, true);
  }
  const issues = issuesOf(space.id);
  const items = issues.map((issue) => {
    const steps = issue.timeline.map((t) => `
      <div class="tl-step">
        <span class="tl-icon i-${t.step}">${STEP_ICON[t.step]}</span>
        <div class="tl-body"><b>${t.label}</b><span>${t.actor}</span></div>
        <span class="ts">${t.ts}</span>
      </div>`).join('<div class="tl-line"></div>');
    return `
      <div class="feed-card clickable" onclick="openIssue('${issue.id}')">
        <div class="fc-title">${issue.title}</div>
        <div class="timeline">${steps}</div>
      </div>`;
  }).join('');
  return feedSection('이슈 흐름', 'Issue Flow', items, false);
}

function reuseFeedHTML(space, guest) {
  const events = reuseEventsOf(space.id);
  const items = events.map((r) => {
    const k = knowledgeById(r.knowledgeId);
    const producer = spaceById(k.spaceId);
    const consumer = spaceById(r.consumerSpace);
    const outbound = k.spaceId === space.id;
    const dirText = outbound
      ? `이 방의 지식 → <b>${consumer.name}</b>에서 재사용`
      : `<b>${producer.name}</b>의 지식을 재사용`;
    const chain = r.chain.map((c, i) => `
      ${i > 0 ? '<span class="chain-arrow">↓</span>' : ''}
      <div class="chain-step">
        <b>${c.label}</b>
        <span>${guest ? (i === r.chain.length - 1 ? consumer.name : producer.name) : c.actor} · ${c.ts}</span>
      </div>`).join('');
    return `
      <div class="feed-card reuse-card ${outbound ? 'outbound' : 'inbound'}">
        <div class="reuse-dir">${dirText}</div>
        <button class="k-link" onclick="openKnowledge('${k.id}')">📄 ${k.title}</button>
        ${r.estSavedTokens ? `<div class="reuse-saved">약 ~${Math.round(r.estSavedTokens / 1000)}k 토큰 · ~${r.estSavedMinutes}분 절약</div>` : ''}
        <div class="chain">${chain}</div>
      </div>`;
  }).join('');
  return feedSection('지식 재사용', 'Knowledge Reuse', items, false,
    guest ? '지식 문서는 조직 공개 — 게스트도 열람 가능' : '');
}

function activityFeedHTML(agents, guest) {
  const online = agents.filter((a) => a.status !== 'offline').length;
  const items = agents.map((a) => `
    <div class="agent-row ${a.status === 'offline' ? 'off' : ''}">
      <span class="agent-dot role-${a.role}"></span>
      <span class="agent-name">${guest ? ROLE_LABEL[a.role] + ' 에이전트' : a.name}</span>
      <span class="agent-status">${guest ? STATUS_LABEL[a.status] : (a.statusLine || STATUS_LABEL[a.status])}</span>
    </div>`).join('');
  return feedSection('Agent Activity', `${online} Agents Online`, items, false);
}

function feedSection(title, sub, items, frosted, note) {
  return `
    <section class="feed ${frosted ? 'frosted' : ''}">
      <div class="feed-head"><h3>${title}</h3><span class="feed-sub">${sub}</span></div>
      ${note ? `<p class="feed-note">${note}</p>` : ''}
      ${items || '<p class="muted small">표시할 항목이 없어요</p>'}
    </section>`;
}

// ── 대시보드 (GET /stats → 팀 리더용 집계, 04-data-mapping §stats) ──

function openDashboard() {
  const s = DB.stats;
  const spaceName = (id) => (spaceById(id) || { name: id }).name;
  const maxFlow = Math.max(...s.bySpace.map((b) => Math.max(b.contributed, b.reused)));
  const bars = s.bySpace.map((b) => `
    <div class="dash-row">
      <span class="dash-name">${spaceName(b.spaceId)}</span>
      <div class="dash-bars">
        <div class="dash-bar give" style="width:${(b.contributed / maxFlow) * 100}%">${b.contributed}</div>
        <div class="dash-bar take" style="width:${(b.reused / maxFlow) * 100}%">${b.reused}</div>
      </div>
    </div>`).join('');
  const rank = (items, label) => items.map((x, i) => `
    <li><span class="rank-n">${i + 1}</span> ${x.title || x.name} <span class="ts">${label} ${x.reuseCount}</span></li>`).join('');
  showModal(`
    <div class="doc-head">
      <span class="chip chip-org">집계 · ${s.period.from} ~ ${s.period.to}</span>
      <h2>조직 대시보드</h2>
      <p class="doc-meta">서버 결정론 집계 — 절약치는 추정(~)으로만 표기</p>
    </div>
    <div class="org-stats dash-totals">
      <div class="org-stat"><b>${s.totals.issues}</b><span>이슈</span></div>
      <div class="org-stat"><b>${s.totals.knowledge}</b><span>지식</span></div>
      <div class="org-stat"><b>${s.totals.reuses}</b><span>재사용</span></div>
      <div class="org-stat"><b>${s.totals.skills}</b><span>Skill</span></div>
    </div>
    <p class="dash-saved">기간 내 절약 추정 <b>약 ~${Math.round(s.tokensSavedEst / 1000)}k 토큰</b></p>
    <div class="doc-section"><h4>스페이스별 기여 ↔ 소비</h4>
      <p class="muted small">위 = 다른 팀이 가져간 지식(기여) · 아래 = 가져와 쓴 지식(소비)</p>
      <div class="dash-chart">${bars}</div>
    </div>
    <div class="doc-section"><h4>Top 재사용 Skill</h4><ul class="rank-list">${rank(s.topReusedSkills, '재사용')}</ul></div>
    <div class="doc-section"><h4>Top 지식</h4><ul class="rank-list">${rank(s.topKnowledge, '인용')}</ul></div>
  `);
}

// ── 모달 (F6) ─────────────────────────────────────────────────────

// visibility:'space' 문서는 로비 tier에선 아예 미노출, 방 게스트 tier에선 서버가 body 없이
// title만 내려준다 (04 §게스트). 이 가드는 딥링크·인용 경유 접근에 대한 방어용.
const knowledgeLocked = (k) => k.visibility === 'space' && roleFor(k.spaceId) !== 'member';

function openKnowledge(id) {
  const k = knowledgeById(id);
  const space = spaceById(k.spaceId);
  if (knowledgeLocked(k)) {
    showModal(`
      <div class="doc-head">
        <span class="chip chip-space">스페이스 전용</span>
        <h2>🔒 ${k.title}</h2>
        <p class="doc-meta">${space.name} · 멤버만 원문을 열람할 수 있어요</p>
      </div>
      <p class="doc-summary muted">이 문서는 ${space.name} 내부용으로 공유됐어요.
        요약과 원문은 스페이스 멤버에게만 보여요. 필요하면 ${space.name}에 공유(새니타이징 후 조직 공개)를 요청하세요.</p>
      <div class="doc-section"><h4>재사용 이력</h4><p class="muted small">${k.citedBy.length}회 인용 — 상세는 멤버 전용</p></div>
    `);
    return;
  }
  const cited = k.citedBy.length
    ? k.citedBy.map((c) => `<li><b>${spaceById(c.spaceId).name}</b> — “${c.issueTitle}” <span class="ts">${c.ts}</span></li>`).join('')
    : '<li class="muted">아직 인용 기록이 없어요</li>';
  showModal(`
    <div class="doc-head">
      <span class="chip ${k.visibility === 'org' ? 'chip-org' : 'chip-space'}">${k.visibility === 'org' ? '조직 공개' : '스페이스 전용'}</span>
      <h2>${k.title}</h2>
      <p class="doc-meta">${k.author} · ${space.name} · ${k.ts}</p>
    </div>
    <p class="doc-summary">${k.summary}</p>
    ${Object.entries(k.body).map(([sec, text]) => `
      <div class="doc-section"><h4>${sec}</h4><p>${text}</p></div>`).join('')}
    <div class="doc-section"><h4>재사용 이력 (${k.citedBy.length})</h4><ul class="cited-list">${cited}</ul></div>
  `);
}

function openIssue(id) {
  const issue = DB.issues.find((i) => i.id === id);
  if (roleFor(issue.spaceId) !== 'member') return;
  const steps = issue.timeline.map((t) => `
    <div class="tl-step big">
      <span class="tl-icon i-${t.step}">${STEP_ICON[t.step]}</span>
      <div class="tl-body"><b>${t.label} — ${t.actor}</b><span>${t.note}</span></div>
      <span class="ts">${t.ts}</span>
    </div>`).join('<div class="tl-line tall"></div>');
  showModal(`
    <div class="doc-head">
      <span class="chip chip-${issue.status}">${issue.status === 'resolved' ? '해결 완료' : '진행 중'}</span>
      <h2>${issue.title}</h2>
      <p class="doc-meta">${spaceById(issue.spaceId).name} · 멤버 전용 상세</p>
    </div>
    <div class="timeline modal-tl">${steps}</div>
  `);
}

function showModal(inner) {
  $modal().innerHTML = `
    <div class="modal-backdrop" onclick="if(event.target===this)closeModal()">
      <div class="modal">
        <button class="modal-x" onclick="closeModal()">×</button>
        ${inner}
      </div>
    </div>`;
}

function closeModal() { $modal().innerHTML = ''; }

document.addEventListener('keydown', (e) => { if (e.key === 'Escape') closeModal(); });

// 딥링크: index.html#space/sw-innov 또는 #space/sw-innov/guest
const hash = location.hash.match(/^#space\/([\w-]+)(\/guest)?/);
if (hash && spaceById(hash[1])) {
  enterSpace(hash[1]);
  if (hash[2]) { state.forceGuest = true; render(); }
} else {
  render();
}
