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

function esc(s: string): string {
  return s.replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' })[c]!)
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

function fitScene() {
  if (!currentScene || !appReady) return
  const b = currentScene.getLocalBounds()
  const margin = 80
  const s = Math.min(
    (app.screen.width - margin) / b.width,
    (app.screen.height - margin - 60) / b.height,
  )
  currentScene.scale.set(s)
  currentScene.position.set(
    (app.screen.width - b.width * s) / 2 - b.x * s,
    (app.screen.height - b.height * s) / 2 - b.y * s + 24,
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
  sceneHost.style.display = 'none'

  let floors: LobbyFloor[] = []
  let loadError = ''
  try {
    floors = await getFloors()
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

function openIssuesPanel(data: SpaceView) {
  const html = data.issues.length
    ? data.issues
        .map(
          (i) => `
        <div class="issue-item">
          <div>${statusBadge(i.status)} <b>${esc(i.title)}</b></div>
          <ul class="timeline">
            ${i.timeline.map((t) => `<li><b>${esc(t.label)}</b> — ${esc(t.actor)}<br/><span class="muted">${esc(t.note)}</span></li>`).join('')}
          </ul>
        </div>`,
        )
        .join('')
    : '<div class="muted">이슈가 없습니다.</div>'
  showPanel('칠판 — 이슈', html)
}

function openKnowledgePanel(data: SpaceView) {
  const html = data.knowledge.length
    ? data.knowledge
        .map(
          (d, i) => `
        <div class="doc-item" data-doc="${i}">
          <b>${esc(d.title)}</b>
          <div class="muted">${esc(d.author_agent)} · 재사용 ${d.reuse_count}</div>
          <div class="doc-summary">${esc(d.summary)}</div>
        </div>`,
        )
        .join('')
    : '<div class="muted">등록된 지식이 없습니다.</div>'
  showPanel('책장 — 지식', html)
  panelBody.querySelectorAll<HTMLElement>('.doc-item').forEach((el) => {
    el.addEventListener('click', () => {
      const doc = data.knowledge[Number(el.dataset.doc)]
      if (doc) showModal(doc.title, `<pre class="doc-body">${esc(doc.body)}</pre>`)
    })
  })
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

  const title = $('room-title')
  const floors = floorsCache
  const liveName = floors?.find((f) => f.space_id === spaceId)?.name
  if (liveName) title.textContent = liveName
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
      onBoardTap: () => openIssuesPanel(data),
      onShelfTap: () => openKnowledgePanel(data),
    },
  )
  app.stage.addChild(currentScene)
  fitScene()
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
