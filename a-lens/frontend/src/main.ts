// A-Lens 프론트 진입점 — 홈(방 목록) / 방 만들기(빌더) / 방 안(PixiJS 씬) 라우팅.
// 로비(사옥) 화면은 보류 — 이슈 #44 순서 조정. 데이터는 백엔드 뷰모델만 사용 (ADR 0003).

import { Application, Container } from 'pixi.js'
import { fetchLobby, fetchSpace, type LobbyFloor, type SpaceView } from './api'
import { openBuilder } from './builder'
import { loadKit } from './room/kit'
import { buildRoomScene } from './room/renderer'
import type { RoomConfig } from './room/types'
import { deleteRoom, getRoom, loadRooms, saveRoom } from './store'

const $ = <T extends HTMLElement>(id: string): T => {
  const el = document.getElementById(id)
  if (!el) throw new Error(`#${id} 엘리먼트 없음`)
  return el as T
}

const sceneHost = $('scene')
const homeEl = $('home')
const headerEl = $('room-header')
const panel = $('panel')
const panelTitle = $('panel-title')
const panelBody = $('panel-body')
const modal = $('modal')
const hub = $('hub')
const hubTabs = $('hub-tabs')
const hubBody = $('hub-body')
const hubOpen = $('hub-open')

function esc(s: string): string {
  return s.replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' })[c]!)
}

// 시각은 서버가 UTC(ISO)로 준다. 화면에는 KST(+9)로 HH:MM 표시 — 한국 사용자 기준.
function kstTime(iso: string | null | undefined): string {
  if (!iso) return ''
  const d = new Date(iso)
  if (Number.isNaN(d.getTime())) return ''
  return new Intl.DateTimeFormat('ko-KR', {
    timeZone: 'Asia/Seoul', hour: '2-digit', minute: '2-digit', hour12: false,
  }).format(d)
}

function showPanel(title: string, html: string) {
  panelTitle.textContent = title
  panelBody.innerHTML = html
  panel.hidden = false
}
$('panel-close').addEventListener('click', () => (panel.hidden = true))

function showModal(title: string, bodyHtml: string) {
  modal.innerHTML = `
    <div class="modal-card">
      <header class="builder-head"><h2>${esc(title)}</h2>
        <button class="icon-btn" data-act="close">✕</button></header>
      <div class="modal-body">${bodyHtml}</div>
    </div>`
  modal.hidden = false
  modal.querySelector('[data-act="close"]')!.addEventListener('click', () => (modal.hidden = true))
}

// ── Pixi 앱 (방 씬 전용, 홈에서는 숨김) ──
const app = new Application()
let appReady = false
let currentScene: Container | null = null

async function ensureApp() {
  if (appReady) return
  await app.init({ resizeTo: window, background: '#0d1220', antialias: true })
  sceneHost.appendChild(app.canvas)
  appReady = true
  window.addEventListener('resize', () => fitScene())
}

const HUB_W = 340 // #hub 사이드바 폭 — 씬 가용 영역에서 제외
const SCENE_PAD = 12 // 잘림 방지용 최소 여백 (px)
function fitScene() {
  if (!currentScene || !appReady) return
  const b = currentScene.getLocalBounds()
  // 가용 영역 = 전체 화면 − (열린 Hub 폭) − 상단 헤더 높이. 여백은 최소만 두고 방을 꽉 채운다.
  const headerH = headerEl.hidden ? 0 : headerEl.offsetHeight
  const availW = app.screen.width - (hub.hidden ? 0 : HUB_W) - SCENE_PAD * 2
  const availH = app.screen.height - headerH - SCENE_PAD * 2
  // contain: 잘림 없이 가용 영역에 최대로 — 가로/세로 배율 중 작은 쪽.
  const s = Math.min(availW / b.width, availH / b.height)
  currentScene.scale.set(s)
  // 가용 영역(헤더 아래 · Hub 왼쪽) 중앙에 배치.
  currentScene.position.set(
    SCENE_PAD + (availW - b.width * s) / 2 - b.x * s,
    headerH + SCENE_PAD + (availH - b.height * s) / 2 - b.y * s,
  )
}

// ── 데이터 캐시 ──
let floorsCache: LobbyFloor[] | null = null
async function getFloors(): Promise<LobbyFloor[]> {
  if (!floorsCache) floorsCache = (await fetchLobby()).floors
  return floorsCache
}

// ── 홈 화면: 만든 방 목록 + 방 만들기 ──
async function renderHome() {
  homeEl.hidden = false
  headerEl.hidden = true
  panel.hidden = true
  modal.hidden = true
  hub.hidden = true
  hubOpen.hidden = true
  sceneHost.style.display = 'none'

  let floors: LobbyFloor[] = []
  let loadError = ''
  try {
    // 홈에 올 때마다 새로 조회 — 허브 실데이터의 최신 스탯 반영 (서버 30s 캐시)
    floorsCache = (await fetchLobby()).floors
    floors = floorsCache
  } catch (e) {
    loadError = String(e)
  }

  const rooms = loadRooms()
  const roomCards = rooms
    .map((r) => {
      const f = floors.find((f) => f.space_id === r.space_id)
      const stats = f ? `지식 ${f.stats.knowledge ?? 0} · 재사용 ${f.stats.reuse ?? 0} · 해결 ${f.stats.resolved ?? 0}` : ''
      return `
      <div class="room-card" data-room="${esc(r.space_id)}">
        <div class="room-card-name">${esc(f?.name ?? r.space_name)}</div>
        <div class="room-card-stats">${stats}</div>
        <div class="room-card-actions">
          <button class="primary-btn sm" data-act="enter">입장</button>
          <button class="ghost-btn sm" data-act="edit">꾸미기</button>
          <button class="ghost-btn sm" data-act="del">삭제</button>
        </div>
      </div>`
    })
    .join('')

  const unbuilt = floors.filter((f) => !getRoom(f.space_id))
  const unbuiltCards = unbuilt
    .map(
      (f) => `
      <div class="room-card dim" data-space="${esc(f.space_id)}">
        <div class="room-card-name">${esc(f.name)}</div>
        <div class="room-card-stats">아직 방 없음</div>
        <div class="room-card-actions"><button class="ghost-btn sm" data-act="build">이 스페이스로 방 만들기</button></div>
      </div>`,
    )
    .join('')

  homeEl.innerHTML = `
    <div class="home-wrap">
      <header class="home-head">
        <h1>A-Lens</h1>
        <p class="home-sub">스페이스 방 관전 — 방을 만들고 a-hub 데이터를 들여다보세요</p>
        <button class="primary-btn" id="btn-new-room" ${floors.length ? '' : 'disabled'}>+ 방 만들기</button>
      </header>
      ${loadError ? `<div class="error-note">백엔드 연결 실패: ${esc(loadError)}</div>` : ''}
      ${rooms.length ? `<h3 class="home-section">내가 만든 방</h3><div class="card-grid">${roomCards}</div>` : ''}
      ${unbuilt.length ? `<h3 class="home-section">방이 없는 스페이스</h3><div class="card-grid">${unbuiltCards}</div>` : ''}
      ${!rooms.length && !unbuilt.length && !loadError ? '<div class="error-note">표시할 스페이스가 없습니다.</div>' : ''}
    </div>`

  const startBuilder = (initial?: RoomConfig, presetSpaceId?: string) => {
    void openBuilder({
      floors: presetSpaceId ? floors.filter((f) => f.space_id === presetSpaceId).concat(floors.filter((f) => f.space_id !== presetSpaceId)) : floors,
      initial,
      onSaved: (config) => {
        saveRoom(config)
        location.hash = `#room/${config.space_id}`
      },
    })
  }

  homeEl.querySelector('#btn-new-room')?.addEventListener('click', () => startBuilder())
  homeEl.querySelectorAll<HTMLElement>('.room-card[data-room]').forEach((card) => {
    const id = card.dataset.room!
    card.querySelector('[data-act="enter"]')?.addEventListener('click', () => (location.hash = `#room/${id}`))
    card.querySelector('[data-act="edit"]')?.addEventListener('click', () => startBuilder(getRoom(id)))
    card.querySelector('[data-act="del"]')?.addEventListener('click', () => {
      deleteRoom(id)
      void renderHome()
    })
  })
  homeEl.querySelectorAll<HTMLElement>('.room-card[data-space]').forEach((card) => {
    card.querySelector('[data-act="build"]')?.addEventListener('click', () => startBuilder(undefined, card.dataset.space))
  })
}

// ── 방 안 화면 ──
function statusBadge(status: string): string {
  const map: Record<string, [string, string]> = {
    open: ['열림', '#d08770'],
    knowledge_linked: ['지식 연결', '#7fb3d5'],
    resolved: ['해결', '#a3be8c'],
  }
  const [label, color] = map[status] ?? [status, '#888']
  return `<span class="badge" style="border-color:${color};color:${color}">${esc(label)}</span>`
}

// 서랍장(책장) = "재사용하면 좋을 주요 지식". 지금은 조직 공개(org) 지식을 앞으로 모아
// 전량 노출한다. TODO: 파트 공용에 도움되는 지식(예: 공용 서버 변경) 분류는 추후 a-lens
// 백엔드에서 허브 데이터를 분석해 산출한다 (재사용 추천 점수). 그 전까지 visibility로 근사.
function openKnowledgePanel(data: SpaceView) {
  const docs = data.knowledge
    .map((d, i) => ({ d, i }))
    .sort((a, b) => (a.d.visibility === 'org' ? 0 : 1) - (b.d.visibility === 'org' ? 0 : 1))
  const html = docs.length
    ? docs
        .map(
          ({ d, i }) => `
        <div class="doc-item" data-doc="${i}">
          <b>${esc(d.title)}</b>
          <div class="muted">${esc(d.author_agent)}${d.visibility === 'org' ? ' · 조직 공개' : ''} · 재사용 ${d.reuse_count}</div>
          <div class="doc-summary">${esc(d.summary)}</div>
        </div>`,
        )
        .join('')
    : '<div class="muted">등록된 지식이 없습니다.</div>'
  showPanel('책장 — 재사용하면 좋을 지식', html)
  panelBody.querySelectorAll<HTMLElement>('.doc-item').forEach((el) => {
    el.addEventListener('click', () => {
      const doc = data.knowledge[Number(el.dataset.doc)]
      if (doc) showModal(doc.title, `<pre class="doc-body">${esc(doc.body)}</pre>`)
    })
  })
}

// ── 오른쪽 Collaboration Hub (상시 사이드바 + 탭) ──
// a-lens는 사람이 보는 view — 캐릭터·Activity는 '사람'이다. 라벨도 사람/팀 관점.
type HubTab = 'all' | 'issues' | 'reuse' | 'activity'
const HUB_TABS: { id: HubTab; label: string; icon: string }[] = [
  { id: 'all', label: '팀 방', icon: '🏠' },
  { id: 'issues', label: '이슈 공유', icon: '🔗' },
  { id: 'reuse', label: '지식 재사용', icon: '📄' },
  { id: 'activity', label: '팀 활동', icon: '👥' },
]
let hubTab: HubTab = 'all'

function hubIssuesHTML(data: SpaceView): string {
  const items = data.issues.length
    ? data.issues
        .map(
          (i) => `
        <div class="feed-card">
          <div class="fc-title">${statusBadge(i.status)} ${esc(i.title)}</div>
          <div class="timeline">
            ${i.timeline
              .map((t) => {
                const at = kstTime(t.at)
                return `<div class="tl-step"><b>${esc(t.label)}</b><span class="tl-actor">${esc(t.actor)}</span>${at ? `<span class="tl-at">${at}</span>` : ''}</div>`
              })
              .join('')}
          </div>
        </div>`,
        )
        .join('')
    : '<p class="muted small">표시할 항목이 없어요</p>'
  return hubSection('이슈 흐름', 'Issue Flow', items)
}

function hubReuseHTML(_data: SpaceView): string {
  // 재사용 이벤트 조회 API가 허브에 아직 없다 (collector reuse_events: []). 데이터가 붙기
  // 전까지 정직하게 빈 상태로 둔다. TODO: a-lens 백엔드에서 재사용 피드 산출 후 연결.
  return hubSection('지식 재사용', 'Knowledge Reuse', '<p class="muted small">표시할 항목이 없어요</p>')
}

function hubActivityHTML(data: SpaceView): string {
  const online = data.agents.filter((a) => a.status === 'working').length
  const items = data.agents.length
    ? data.agents
        .map(
          (a) => {
            const at = kstTime(a.last_active_at)
            const status = a.status_line || (a.status === 'working' ? '활동 중' : at ? `마지막 활동 ${at}` : '자리 비움')
            return `
        <div class="agent-row ${a.status === 'working' ? '' : 'off'}">
          <span class="agent-dot ${a.status === 'working' ? 'on' : ''}"></span>
          <span class="agent-name">${esc(a.name)}</span>
          <span class="agent-status">${esc(status)}</span>
        </div>`
          },
        )
        .join('')
    : '<p class="muted small">표시할 항목이 없어요</p>'
  return hubSection('팀 활동', `${online} Online`, items)
}

function hubSection(title: string, sub: string, items: string): string {
  return `
    <section class="feed">
      <div class="feed-head"><h3>${esc(title)}</h3><span class="feed-sub">${esc(sub)}</span></div>
      ${items}
    </section>`
}

// 사이드바 접힘 상태 — 다음 입장 때도 유지되게 localStorage에 기억.
let hubCollapsed = localStorage.getItem('a-lens.hub.collapsed') === '1'

/** 방 화면에서만 호출 — 접힘 여부에 따라 사이드바/열기버튼 표시를 정하고 씬 폭을 재조정한다. */
function applyHubCollapsed(inRoom: boolean) {
  hub.hidden = !inRoom || hubCollapsed
  hubOpen.hidden = !inRoom || !hubCollapsed
  fitScene()
}

function toggleHub(collapsed: boolean) {
  hubCollapsed = collapsed
  localStorage.setItem('a-lens.hub.collapsed', collapsed ? '1' : '0')
  applyHubCollapsed(true)
}
$('hub-collapse').addEventListener('click', () => toggleHub(true))
hubOpen.addEventListener('click', () => toggleHub(false))

function renderHub(data: SpaceView) {
  hubTabs.innerHTML = HUB_TABS.map(
    (t) => `<button class="hub-tab ${t.id === hubTab ? 'on' : ''}" data-tab="${t.id}">
      <span class="hub-tab-ico">${t.icon}</span><span>${t.label}</span></button>`,
  ).join('')
  hubTabs.querySelectorAll<HTMLElement>('.hub-tab').forEach((btn) => {
    btn.addEventListener('click', () => {
      hubTab = btn.dataset.tab as HubTab
      renderHub(data)
    })
  })

  const sections: Record<HubTab, () => string> = {
    all: () => hubIssuesHTML(data) + hubReuseHTML(data) + hubActivityHTML(data),
    issues: () => hubIssuesHTML(data),
    reuse: () => hubReuseHTML(data),
    activity: () => hubActivityHTML(data),
  }
  hubBody.innerHTML = sections[hubTab]()
  applyHubCollapsed(true)
}

async function renderRoom(spaceId: string) {
  const config = getRoom(spaceId)
  if (!config) {
    location.hash = ''
    return
  }
  homeEl.hidden = true
  panel.hidden = true
  sceneHost.style.display = ''
  await ensureApp()

  headerEl.hidden = false
  headerEl.innerHTML = `
    <button class="ghost-btn sm" id="btn-back">← 목록</button>
    <b id="room-title">${esc(config.space_name)}</b>
    <span class="muted" id="room-visits"></span>
    <span class="spacer"></span>
    <button class="ghost-btn sm" id="btn-edit-room">꾸미기</button>`
  $('btn-back').addEventListener('click', () => (location.hash = ''))
  $('btn-edit-room').addEventListener('click', async () => {
    const floors = await getFloors()
    void openBuilder({
      floors,
      initial: getRoom(spaceId),
      onSaved: (c) => {
        saveRoom(c)
        void renderRoom(spaceId)
      },
    })
  })

  let data: SpaceView
  try {
    data = await fetchSpace(spaceId)
  } catch (e) {
    showPanel('연결 오류', `스페이스 데이터를 불러오지 못했습니다.<br/>${esc(String(e))}`)
    data = { space_id: spaceId, viewer_tier: 'member', agents: [], issues: [], knowledge: [], visits: null }
  }
  if (location.hash !== `#room/${spaceId}` && location.hash !== `#room/${encodeURIComponent(spaceId)}`) {
    return
  }

  const title = $('room-title')
  const floors = floorsCache
  const liveName = floors?.find((f) => f.space_id === spaceId)?.name
  if (liveName) {
    title.textContent = liveName
    config.space_name = liveName // 칠판 위 간판에도 최신 이름 반영
  }
  $('room-visits').textContent = data.visits ? `방문 TODAY ${data.visits.today} · TOTAL ${data.visits.total}` : ''

  if (currentScene) {
    currentScene.destroy({ children: true })
    currentScene = null
  }
  app.stage.removeChildren()
  currentScene = buildRoomScene(
    config,
    { agents: data.agents, issues: data.issues, knowledgeCount: data.knowledge.length },
    {
      onAgentTap: (agent) =>
        showPanel(
          `🤖 ${agent.name}`,
          `<div><b>역할</b> ${esc(agent.role || '-')}</div>
           <div><b>소유자</b> ${esc(agent.owner || '-')}</div>
           <div><b>상태</b> ${agent.status === 'working' ? '🟢 일하는 중' : '⚪ 자리 비움'}</div>
           <div class="doc-summary">${esc(agent.status_line || '')}</div>`,
        ),
      // 칠판(이슈)은 오른쪽 Hub "이슈 흐름"으로 흡수 — 클릭 팝업 제거 (2026-07-19).
      onShelfTap: () => openKnowledgePanel(data),
    },
  )
  app.stage.addChild(currentScene)

  // 오른쪽 Collaboration Hub — 내용 채우고, 접힘 상태에 맞춰 표시 + 씬 폭 재조정(fitScene).
  hubTab = 'all'
  renderHub(data)
}

// ── 해시 라우팅 ──
function route() {
  const m = location.hash.match(/^#room\/(.+)$/)
  if (m) void renderRoom(decodeURIComponent(m[1]))
  else void renderHome()
}
window.addEventListener('hashchange', route)
// 스프라이트 킷(있으면)을 먼저 로드하고 첫 라우팅 — 없으면 Graphics 폴백
void loadKit().then(route)
