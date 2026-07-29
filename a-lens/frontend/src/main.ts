// A-Lens 프론트 진입점 — 홈(방 목록) / 방 만들기(빌더) / 방 안(PixiJS 씬) 라우팅.
// 로비(사옥) 화면은 보류 — 이슈 #44 순서 조정. 데이터는 백엔드 뷰모델만 사용 (ADR 0003).

import DOMPurify from 'dompurify'
import { marked } from 'marked'
import { Application, Container } from 'pixi.js'
import {
  fetchLobby,
  fetchSettings,
  fetchSpace,
  fetchRoomChat,
  type RoomChatSpot,
  saveSettings,
  testConnections,
  type KnowledgeDoc,
  type LobbyFloor,
  type SpaceAgent,
  type SpaceIssue,
  type SpaceView,
} from './api'
import { openBuilder } from './builder'
import { resolveLifeId } from './life/catalog'
import { loadKit } from './life/kit'
import { buildLifeScene } from './life/renderer'
import type { LifeConfig } from './life/types'
import { deleteLife, getLife, loadLife, loadRooms, saveLife } from './store'

const $ = <T extends HTMLElement>(id: string): T => {
  const el = document.getElementById(id)
  if (!el) throw new Error(`#${id} 엘리먼트 없음`)
  return el as T
}

const sceneHost = $('scene')
const homeEl = $('home')
const headerEl = $('life-header')
const panel = $('panel')
const panelTitle = $('panel-title')
const panelBody = $('panel-body')
const modal = $('modal')
const hub = $('hub')
const hubTabs = $('hub-tabs')
const hubBody = $('hub-body')
const hubActivity = $('hub-activity')
const hubOpen = $('hub-open')

function esc(s: string): string {
  return s.replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' })[c]!)
}

// Page 원문(create_page로 남긴 마크다운)을 모달에 렌더링할 때 씀 — 팀원이 작성한 임의 텍스트라
// DOMPurify로 한 번 걸러서 XSS를 막는다.
function mdHTML(md: string): string {
  return DOMPurify.sanitize(marked.parse(md || '', { async: false }) as string)
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

// 날짜+시간 (KST) — 'YYYY-MM-DD HH:MM'. 상세 패널의 최근 활동 시간처럼 날짜가 필요할 때.
function kstDateTime(iso: string | null | undefined): string {
  if (!iso) return ''
  const d = new Date(iso)
  if (Number.isNaN(d.getTime())) return ''
  const p = new Intl.DateTimeFormat('en-CA', {
    timeZone: 'Asia/Seoul',
    year: 'numeric', month: '2-digit', day: '2-digit',
    hour: '2-digit', minute: '2-digit', hour12: false,
  }).formatToParts(d)
  const g = (t: string) => p.find((x) => x.type === t)?.value ?? ''
  return `${g('year')}-${g('month')}-${g('day')} ${g('hour')}:${g('minute')}`
}

function showPanel(title: string, html: string) {
  cancelTyping()
  panel.classList.remove('panel-pixel')
  panelTitle.textContent = title
  panelBody.innerHTML = html
  positionPanel()
  panel.hidden = false
}
// Hub가 접혀 있으면 패널을 화면 오른쪽 위(right:16px)로, 펼쳐져 있으면 사이드바 왼쪽으로.
function positionPanel() {
  panel.classList.toggle('hub-collapsed', hubCollapsed)
}
$('panel-close').addEventListener('click', () => {
  cancelTyping()
  panel.hidden = true
})

// ── 캐릭터 상세 패널 — 픽셀 글씨체 + 한 글자씩 타이핑 (게임 대사창 느낌) ──
let typingTimer: number | null = null
function cancelTyping() {
  if (typingTimer !== null) {
    clearTimeout(typingTimer)
    typingTimer = null
  }
}

/** 여러 줄(label + value)을 한 글자씩 순차 타이핑한다. 라벨은 즉시, 값만 타이핑. */
function typeLines(lines: { label: string; value: string }[]) {
  cancelTyping()
  panelBody.innerHTML = lines
    .map((l, i) => `<div class="type-line" data-i="${i}"><b>${esc(l.label)}</b> <span class="type-val"></span></div>`)
    .join('')
  const vals = Array.from(panelBody.querySelectorAll<HTMLElement>('.type-val'))
  const caret = document.createElement('span')
  caret.className = 'type-caret'

  let li = 0
  let ci = 0
  const SPEED = 14 // ms/글자 (타이핑 속도 — 낮을수록 빠름)
  const step = () => {
    if (li >= lines.length) {
      caret.remove()
      typingTimer = null
      return
    }
    const target = lines[li].value
    const span = vals[li]
    if (ci === 0) span.after(caret) // 현재 줄 끝에 커서
    if (ci < target.length) {
      span.textContent = target.slice(0, ci + 1)
      ci++
      typingTimer = window.setTimeout(step, SPEED)
    } else {
      li++
      ci = 0
      if (li < lines.length) vals[li].after(caret)
      typingTimer = window.setTimeout(step, SPEED * 3) // 줄바꿈은 살짝 쉼
    }
  }
  step()
}

// ── 창문 말풍선 — 창밖 잡담 2~3턴 자동 재생 (패널과 같은 픽셀 폰트·타이핑) ──
// 스펙: docs/design/a-lens/specs/2026-07-28-room-object-smalltalk-design.md
const windowBubble = $('window-bubble')
let bubbleTimer: number | null = null
let bubbleSeq = 0 // 재생 세대 — 새 클릭·취소가 이전 재생의 콜백을 무효화한다

function hideWindowBubble() {
  if (bubbleTimer !== null) {
    clearTimeout(bubbleTimer)
    bubbleTimer = null
  }
  bubbleSeq++
  windowBubble.hidden = true
  windowBubble.innerHTML = ''
}

/** 한 줄을 한 글자씩 찍고 done 호출. 세대(seq)가 바뀌면 조용히 멈춘다. */
function typeBubbleLine(el: HTMLElement, text: string, seq: number, done: () => void) {
  const SPEED = 45 // ms/글자 — 대사는 패널 정보보다 느리게 읽히는 속도로
  const caret = document.createElement('span')
  caret.className = 'type-caret'
  el.after(caret)
  let ci = 0
  const step = () => {
    if (seq !== bubbleSeq) return
    if (ci < text.length) {
      el.textContent = text.slice(0, ++ci)
      bubbleTimer = window.setTimeout(step, SPEED)
    } else {
      caret.remove()
      done()
    }
  }
  step()
}

// 대사 선입 큐 — LLM 생성이 4초쯤 걸려서, 방에 들어올 때 미리 한 세트를 받아둔다.
// 클릭 즉시 재생되고, 재생을 시작하면 다음 세트를 백그라운드로 채운다.
const chatQueue: Record<RoomChatSpot, string[][]> = { window: [], water: [] }
let chatView = '' // 방(프리셋)이 바뀌면 큐를 버린다 — 창밖 풍경이 달라지므로

function prefetchRoomChat(spot: RoomChatSpot, view: string, spaceName: string) {
  if (chatView !== view) {
    chatView = view
    chatQueue.window = []
    chatQueue.water = []
  }
  if (chatQueue[spot].length >= 2) return
  void fetchRoomChat(spot, view, spaceName)
    .then((r) => {
      if (chatView === view && r.lines?.length) chatQueue[spot].push(r.lines)
    })
    .catch((e) => console.warn(`${spot} 대사 선입 실패 (클릭 시 재시도)`, e))
}

/** 창문·정수기 클릭 진입점 — 대사를 받아 한 줄씩 이어서 재생한다. 실패해도 말풍선은 뜬다. */
async function playRoomChat(
  spot: RoomChatSpot,
  at: { x: number; y: number },
  view: string,
  spaceName: string,
) {
  hideWindowBubble()
  const seq = ++bubbleSeq
  windowBubble.style.left = `${at.x}px`
  windowBubble.style.top = `${at.y - 8}px` // 창틀에서 살짝 띄운다
  windowBubble.hidden = false
  windowBubble.innerHTML = '<div class="bubble-line muted">…</div>'

  let lines = chatView === view ? (chatQueue[spot].shift() ?? []) : []
  if (!lines.length) {
    try {
      lines = (await fetchRoomChat(spot, view, spaceName)).lines
    } catch (e) {
      console.warn(`${spot} 대사 조회 실패 — 기본 문구로 대체`, e)
      lines =
        spot === 'water'
          ? ['물 한 잔 하고 하세요', '힘들죠? 좀 쉬었다 합시다']
          : ['창밖 좀 보세요', '이런 날은 밖에 있어야 하는데']
    }
  }
  prefetchRoomChat(spot, view, spaceName) // 다음 클릭용 채우기
  if (seq !== bubbleSeq) return // 그 사이 다른 클릭·취소가 있었으면 버린다
  if (!lines.length) lines = ['창밖 좀 보세요']

  playLines(lines, seq)
}

/** 이미 열린 말풍선에 줄들을 순차 타이핑 — 창문·책장 배지가 함께 쓴다. */
function playLines(lines: string[], seq: number) {
  windowBubble.innerHTML = ''
  const HOLD = 2400 // 턴 사이 쉼(ms) — 읽을 시간
  let i = 0
  const next = () => {
    if (seq !== bubbleSeq) return
    if (i >= lines.length) {
      bubbleTimer = window.setTimeout(() => seq === bubbleSeq && hideWindowBubble(), 4000)
      return
    }
    windowBubble.querySelectorAll('.bubble-line').forEach((el) => el.classList.add('past'))
    const el = document.createElement('div')
    el.className = 'bubble-line'
    windowBubble.appendChild(el)
    const text = lines[i++]
    typeBubbleLine(el, text, seq, () => {
      bubbleTimer = window.setTimeout(next, HOLD)
    })
  }
  next()
}

/** 말풍선을 특정 화면 좌표에 열고 주어진 줄을 재생한다 (LLM 없이 즉시 — 결정론 문구). */
function showBubbleAt(at: { x: number; y: number }, lines: string[]) {
  hideWindowBubble()
  const seq = ++bubbleSeq
  windowBubble.style.left = `${at.x}px`
  windowBubble.style.top = `${at.y - 8}px`
  windowBubble.hidden = false
  playLines(lines, seq)
}

/** 책장 문서 건수 배지 클릭 — 쌓인 문서량을 자랑하고 문서함으로 안내한다.
 *  건수는 사실이라 LLM을 쓰지 않는다(수치는 결정론, 스펙 §1의 근거 원칙과 같은 결). */
function playShelfBadgeChat(at: { x: number; y: number }, count: number) {
  showBubbleAt(at, [`벌써 문서가 ${count}건이나 쌓였네요`, '오른쪽 문서함에서 확인해 보세요'])
}

// 취소 규칙(스펙 §7) — 다른 곳 클릭 · Esc · 리사이즈에서 말풍선을 남기지 않는다.
// 씬 리사이즈는 배율이 바뀌어 좌표가 어긋나므로 무조건 닫는다.
window.addEventListener('pointerdown', (e) => {
  if (!windowBubble.hidden && !(e.target as HTMLElement)?.closest('#scene')) hideWindowBubble()
})
window.addEventListener('keydown', (e) => {
  if (e.key === 'Escape') hideWindowBubble()
})
window.addEventListener('resize', () => hideWindowBubble())

function showAgentPanel(agent: SpaceAgent) {
  cancelTyping()
  panel.classList.add('panel-pixel')
  panelTitle.textContent = `🙂 ${agent.name}`
  positionPanel()
  panel.hidden = false
  // 허브 실데이터엔 이름·상태·최근 활동(내용+시각)만 있다. 역할/소유자는 (있으면=데모) 조건부로만.
  const lines = [
    { label: '상태', value: agent.status === 'working' ? '🟢 활동 중' : '⚪ 자리 비움' },
  ]
  if (agent.role) lines.unshift({ label: '역할', value: agent.role })
  if (agent.owner) lines.unshift({ label: '소유자', value: agent.owner })
  if (agent.status_line) lines.push({ label: '', value: agent.status_line })
  // 최근 a-hub 활동을 사람이 읽기 쉽게 풀어쓴 설명 + 그 활동을 한 시각(KST).
  if (agent.recent_activity?.detail) lines.push({ label: '최근 활동', value: agent.recent_activity.detail })
  const lastAt = kstDateTime(agent.last_active_at)
  if (lastAt) lines.push({ label: '최근 활동 시간', value: `${lastAt} (KST)` })
  typeLines(lines)
}

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

// ── 설정 창 — a-hub 연결 · LLM API 구성 (기본값은 서버가 채워 내려줌) ──
async function openSettings() {
  let s
  try {
    s = await fetchSettings()
  } catch (e) {
    showModal('설정', `<div class="error-note">설정을 불러오지 못했습니다: ${esc(String(e))}</div>`)
    return
  }
  // 비밀값 입력칸 — 저장돼 있으면 placeholder로 '설정됨', 바꿀 때만 입력받는다(빈 값=유지).
  const secret = (id: string, label: string, isSet: boolean) => `
    <label class="set-field"><span>${label}</span>
      <input id="${id}" type="password" autocomplete="off"
        placeholder="${isSet ? '설정됨 — 바꿀 때만 입력' : '미설정'}" /></label>`
  const sourceOpts = ['auto', 'hub', 'dummy', 'fixtures']
    .map((o) => `<option value="${o}" ${o === s.source ? 'selected' : ''}>${o}</option>`)
    .join('')

  showModal(
    '설정',
    `<form class="settings-form" id="settings-form" autocomplete="off">
      <h3 class="set-group">a-hub 연결</h3>
      <label class="set-field"><span>a-hub URL</span>
        <input id="set-work_url" type="text" value="${esc(s.work_url)}" placeholder="https://spacea.msalt.net" /></label>
      ${secret('set-work_api_key', 'a-hub API Key (x-api-key)', s.work_api_key_set)}
      ${secret('set-work_token', 'a-hub Token (Bearer)', s.work_token_set)}
      <label class="set-field"><span>데이터 원천</span><select id="set-source">${sourceOpts}</select></label>

      <h3 class="set-group">LLM API — 분류·요약·서사</h3>
      <label class="set-field"><span>LLM URL (OpenAI 호환)</span>
        <input id="set-llm_url" type="text" value="${esc(s.llm_url)}" placeholder="http://…/v1" /></label>
      <label class="set-field"><span>모델</span>
        <input id="set-llm_model" type="text" value="${esc(s.llm_model)}" /></label>
      ${secret('set-llm_key', 'LLM Key (로컬이면 비워둠)', s.llm_key_set)}
      <label class="set-field"><span>요약 길이 <em class="muted">(바꾸면 전체 재번역)</em></span>
        <select id="set-summary_style">
          <option value="brief" ${s.summary_style === 'brief' ? 'selected' : ''}>간결 (한 문장)</option>
          <option value="normal" ${s.summary_style === 'normal' ? 'selected' : ''}>보통 (2~3문장)</option>
          <option value="detailed" ${s.summary_style === 'detailed' ? 'selected' : ''}>상세 (3~5문장)</option>
        </select></label>

      <h3 class="set-group">고급</h3>
      <label class="set-field"><span>번역 캐시 DB 경로 <em class="muted">(비우면 캐시 끔)</em></span>
        <input id="set-db_path" type="text" value="${esc(s.db_path)}" /></label>
      <label class="set-field"><span>폴링 캐시(초)</span>
        <input id="set-cache_ttl" type="number" min="0" step="1" value="${s.cache_ttl}" /></label>

      <div class="set-actions">
        <button type="button" class="ghost-btn" id="set-test">연결 테스트</button>
        <span class="set-status muted small" id="set-status"></span>
        <span class="spacer"></span>
        <button type="submit" class="primary-btn">저장</button>
      </div>
    </form>`,
  )

  const v = (id: string) => modal.querySelector<HTMLInputElement | HTMLSelectElement>(id)!.value
  const collectPatch = (): Record<string, unknown> => {
    const patch: Record<string, unknown> = {
      work_url: v('#set-work_url'),
      source: v('#set-source'),
      llm_url: v('#set-llm_url'),
      llm_model: v('#set-llm_model'),
      summary_style: v('#set-summary_style'),
      db_path: v('#set-db_path'),
      cache_ttl: v('#set-cache_ttl'),
    }
    for (const [id, key] of [
      ['#set-work_api_key', 'work_api_key'],
      ['#set-work_token', 'work_token'],
      ['#set-llm_key', 'llm_key'],
    ] as const) {
      const val = v(id)
      if (val) patch[key] = val // 비밀값은 입력했을 때만 (빈 값 = 유지)
    }
    return patch
  }
  const status = () => modal.querySelector<HTMLElement>('#set-status')!

  modal.querySelector('#set-test')!.addEventListener('click', async () => {
    status().textContent = '테스트 중…'
    try {
      const r = await testConnections(collectPatch())
      const mark = (x: { ok: boolean; status?: number; error?: string }) =>
        `${x.ok ? '✅' : '❌'}${x.status ? ` (${x.status})` : ''}${x.error ? ` ${esc(x.error)}` : ''}`
      status().innerHTML = `a-hub ${mark(r.hub)} · LLM ${mark(r.llm)}`
    } catch (e) {
      status().textContent = '테스트 실패: ' + String(e)
    }
  })

  modal.querySelector('#settings-form')!.addEventListener('submit', async (ev) => {
    ev.preventDefault()
    status().textContent = '저장 중…'
    try {
      await saveSettings(collectPatch())
      floorsCache = null // 새 설정으로 다시 조회하게 캐시 무효화
      modal.hidden = true
      void renderHome()
    } catch (e) {
      status().textContent = '저장 실패: ' + String(e)
    }
  })
}

// ── Pixi 앱 (방 씬 전용, 홈에서는 숨김) ──
const app = new Application()
let appReady = false
let currentScene: Container | null = null

async function ensureApp() {
  if (appReady) return
  await app.init({
    resizeTo: window,
    background: '#0d1220',
    antialias: true,
    resolution: window.devicePixelRatio || 1,
    autoDensity: true,
  })
  sceneHost.appendChild(app.canvas)
  appReady = true
  // 렌더러가 실제로 리사이즈된 뒤(screen.width 갱신 후) 씬을 다시 맞춘다.
  // window resize만 듣던 이전 방식은 resizeTo의 반영 타이밍과 어긋나 배율이 안 맞았다.
  app.renderer.on('resize', () => fitScene())
  window.addEventListener('resize', () => fitScene())
  watchDevicePixelRatio()
}

// 브라우저 줌·모니터 이동 등으로 devicePixelRatio가 바뀌어도 렌더러 resolution은
// init 시점 값에 고정된 채라(리사이즈 이벤트가 안 따라옴) 글자·스프라이트가 흐려진다.
// matchMedia로 현재 DPR을 감시하다가 바뀌면 renderer.resolution을 다시 맞춘다.
function watchDevicePixelRatio() {
  const mq = matchMedia(`(resolution: ${window.devicePixelRatio}dppx)`)
  const onChange = () => {
    app.renderer.resolution = window.devicePixelRatio || 1
    app.renderer.resize(app.screen.width, app.screen.height)
    fitScene()
    watchDevicePixelRatio() // 새 DPR 기준으로 감시자 재등록 (matchMedia는 1회성)
  }
  mq.addEventListener('change', onChange, { once: true })
}

const SCENE_PAD = 12 // 잘림 방지용 최소 여백 (px)
function fitScene() {
  if (!currentScene || !appReady) return
  const b = currentScene.getLocalBounds()
  // 가용 영역 = 전체 화면 − (열린 Hub 실제 폭) − 상단 헤더 높이. 여백은 최소만 두고 방을 꽉 채운다.
  const headerH = headerEl.hidden ? 0 : headerEl.offsetHeight
  // 창이 아주 작아도 가용 영역을 양수로 유지 — 음수 배율(씬 뒤집힘) 방지.
  const hubW = hub.hidden ? 0 : hub.offsetWidth // 리사이즈로 바뀐 현재 폭을 반영
  const availW = Math.max(10, app.screen.width - hubW - SCENE_PAD * 2)
  const availH = Math.max(10, app.screen.height - headerH - SCENE_PAD * 2)
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
// FAKE(더미) 스페이스 표시 여부 — 상단 토글, 새로고침에도 유지되게 localStorage에 기억.
let showFake = true
try {
  showFake = localStorage.getItem('a-lens.show-fake') !== '0'
} catch (e) {
  console.warn('localStorage 읽기 실패 — FAKE 표시 기본값 사용', e)
}

async function renderHome() {
  homeEl.hidden = false
  headerEl.hidden = true
  panel.hidden = true
  cancelTyping() // 상세 패널 타이핑이 진행 중이었으면 홈으로 나갈 때 멈춘다
  hideWindowBubble() // 창문 말풍선도 함께 — 방을 벗어나면 좌표가 의미 없다
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

  // FAKE 숨김이면 더미 스페이스(카드·방 목록 모두)를 화면에서 제외
  const visibleFloors = showFake ? floors : floors.filter((f) => !f.demo)
  const life = loadLife().filter((r) => {
    const f = floors.find((f) => f.space_id === r.space_id)
    return showFake || !f?.demo
  })
  const lifeCards = life
    .map((r) => {
      const f = floors.find((f) => f.space_id === r.space_id)
      const stats = f ? `지식 ${f.stats.knowledge ?? 0} · 재사용 ${f.stats.reuse ?? 0} · 해결 ${f.stats.resolved ?? 0}` : ''
      return `
      <div class="life-card" data-life="${esc(r.space_id)}">
        <div class="life-card-name">${esc(f?.name ?? r.space_name)}${f?.demo ? ' <span class="demo-badge">FAKE</span>' : ''}</div>
        <div class="life-card-stats">${stats}</div>
        <div class="life-card-actions">
          <button class="primary-btn sm" data-act="enter">입장</button>
          <button class="ghost-btn sm" data-act="edit">꾸미기</button>
          <button class="ghost-btn sm" data-act="del">삭제</button>
        </div>
      </div>`
    })
    .join('')

  const unbuilt = visibleFloors.filter((f) => !getLife(f.space_id))
  const unbuiltCards = unbuilt
    .map(
      (f) => `
      <div class="life-card dim" data-space="${esc(f.space_id)}">
        <div class="life-card-name">${esc(f.name)}${f.demo ? ' <span class="demo-badge">FAKE</span>' : ''}</div>
        <div class="life-card-stats">아직 방 없음</div>
        <div class="life-card-actions"><button class="ghost-btn sm" data-act="build">이 스페이스로 방 만들기</button></div>
      </div>`,
    )
    .join('')

  homeEl.innerHTML = `
    <div class="home-wrap">
      <header class="home-head">
        <h1>A-Lens</h1>
        <p class="home-sub">스페이스 방 관전 — 방을 만들고 a-hub 데이터를 들여다보세요</p>
        <button class="primary-btn" id="btn-new-life" ${visibleFloors.length ? '' : 'disabled'}>+ 방 만들기</button>
        ${floors.some((f) => f.demo) ? `<button class="ghost-btn sm fake-toggle ${showFake ? 'on' : ''}" id="btn-toggle-fake">${showFake ? 'FAKE 숨기기' : 'FAKE 보이기'}</button>` : ''}
        <button class="ghost-btn sm" id="btn-settings" title="a-hub·LLM 연결 설정">⚙ 설정</button>
      </header>
      ${loadError ? `<div class="error-note">백엔드 연결 실패: ${esc(loadError)}</div>` : ''}
      ${life.length ? `<h3 class="home-section">내가 만든 방</h3><div class="card-grid">${lifeCards}</div>` : ''}
      ${unbuilt.length ? `<h3 class="home-section">방이 없는 스페이스</h3><div class="card-grid">${unbuiltCards}</div>` : ''}
      ${!life.length && !unbuilt.length && !loadError ? '<div class="error-note">표시할 스페이스가 없습니다.</div>' : ''}
    </div>`

  const startBuilder = (initial?: LifeConfig, presetSpaceId?: string) => {
    void openBuilder({
      floors: presetSpaceId
        ? visibleFloors.filter((f) => f.space_id === presetSpaceId).concat(visibleFloors.filter((f) => f.space_id !== presetSpaceId))
        : visibleFloors,
      initial,
      onSaved: (config) => {
        saveLife(config)
        location.hash = `#life/${config.space_id}`
      },
    })
  }

  homeEl.querySelector('#btn-settings')?.addEventListener('click', () => void openSettings())
  homeEl.querySelector('#btn-new-life')?.addEventListener('click', () => startBuilder())
  homeEl.querySelector('#btn-toggle-fake')?.addEventListener('click', () => {
    showFake = !showFake
    try {
      localStorage.setItem('a-lens.show-fake', showFake ? '1' : '0')
    } catch (e) {
      console.warn('localStorage 쓰기 실패 — FAKE 표시 상태 저장 생략', e)
    }
    void renderHome()
  })
  homeEl.querySelectorAll<HTMLElement>('.life-card[data-life]').forEach((card) => {
    const id = card.dataset.life!
    card.querySelector('[data-act="enter"]')?.addEventListener('click', () => (location.hash = `#life/${id}`))
    card.querySelector('[data-act="edit"]')?.addEventListener('click', () => startBuilder(getLife(id)))
    card.querySelector('[data-act="del"]')?.addEventListener('click', () => {
      deleteLife(id)
      void renderHome()
    })
  })
  homeEl.querySelectorAll<HTMLElement>('.life-card[data-space]').forEach((card) => {
    card.querySelector('[data-act="build"]')?.addEventListener('click', () => startBuilder(undefined, card.dataset.space))
  })
}

// ── 방 안 화면 ──
// 이슈 상태 → 라벨·색 (배지, 행 왼쪽 색 바, 필터 칩이 공유)
const ISSUE_STATUS_META: Record<string, [string, string]> = {
  open: ['열림', '#d08770'],
  knowledge_linked: ['지식 연결', '#7fb3d5'],
  resolved: ['해결', '#a3be8c'],
}
function statusMeta(status: string): [string, string] {
  return ISSUE_STATUS_META[status] ?? [status, '#888']
}
function statusBadge(status: string): string {
  const [label, color] = statusMeta(status)
  return `<span class="badge" style="border-color:${color};color:${color}">${esc(label)}</span>`
}

// LLM 분류 → 사람이 읽을 라벨 + 색 배지 (translator.CATEGORY_KO 미러)
const CATEGORY_KO: Record<string, string> = {
  troubleshoot: '문제해결',
  spec: '설계·스펙',
  analysis: '분석·결과',
  release: '릴리즈·변경',
  note: '노트·회의',
  guide: '가이드',
}
function catBadge(cat?: string | null): string {
  if (!cat) return ''
  return `<span class="cat-badge cat-${esc(cat)}">${esc(CATEGORY_KO[cat] ?? cat)}</span>`
}

// 서랍장(책장) 클릭 → Hub '지식 재사용' 탭으로 전환 (2026-07-19). 탭 안에서
// 재사용 이벤트 피드 + 재사용하면 좋을 지식 목록(클릭 시 원문 모달)을 함께 보여준다.

// ── 오른쪽 Collaboration Hub (상시 사이드바) ──
// a-lens는 사람이 보는 view — 캐릭터·Activity는 '사람'이다. 라벨도 사람/팀 관점.
// 위 2/3 = 탭(이슈 공유 / 지식 재사용) 내용, 아래 1/3 = 팀 활동 상시 표시.
type HubTab = 'issues' | 'reuse' | 'pages'
const HUB_TABS: { id: HubTab; label: string; icon: string }[] = [
  { id: 'issues', label: '이슈 공유', icon: '🔗' },
  { id: 'reuse', label: '지식 재사용', icon: '📄' },
  { id: 'pages', label: '문서함', icon: '📑' },
]
let hubTab: HubTab = 'issues'

// ── 이슈 흐름 탭 — 최신순 고정 + 상태 필터 칩 + 사람 필터 + 시간 버킷 ──
// 보기 "모드"는 화면을 통째로 재배열해 위치 감각을 잃게 한다 — 상태·사람은 필터로,
// 정렬은 항상 최신순으로 고정해 멘탈 모델을 유지한다 (2026-07-19 가독성 개선).
type IssueStatusFilter = 'all' | 'open' | 'knowledge_linked' | 'resolved'
let issueStatusFilter: IssueStatusFilter = 'all'
let issuePersonFilter = '' // '' = 전체

const issueAt = (i: SpaceIssue) => i.timeline?.[i.timeline.length - 1]?.at ?? ''
const issueActor = (i: SpaceIssue) => i.timeline?.[0]?.actor || i.opened_by || '(알 수 없음)'
const byRecent = (a: SpaceIssue, b: SpaceIssue) => issueAt(b).localeCompare(issueAt(a))

// KST 기준 날짜 키 — 시간 버킷(오늘/어제) 판정용
function kstDayKey(d: Date): string {
  return new Intl.DateTimeFormat('en-CA', {
    timeZone: 'Asia/Seoul', year: 'numeric', month: '2-digit', day: '2-digit',
  }).format(d)
}

function timeBucket(iso: string): string {
  if (!iso) return '이전'
  const d = new Date(iso)
  if (Number.isNaN(d.getTime())) return '이전'
  const key = kstDayKey(d)
  if (key === kstDayKey(new Date())) return '오늘'
  if (key === kstDayKey(new Date(Date.now() - 864e5))) return '어제'
  if (d.getTime() >= Date.now() - 7 * 864e5) return '이번 주'
  return '이전'
}

/** 메타 줄의 시각 — 오늘은 HH:MM, 그 외엔 MM-DD HH:MM */
function issueTimeLabel(iso: string): string {
  if (!iso) return ''
  const d = new Date(iso)
  if (Number.isNaN(d.getTime())) return ''
  return kstDayKey(d) === kstDayKey(new Date()) ? kstTime(iso) : kstDateTime(iso).slice(5)
}

// 컴팩트 행: 왼쪽 상태 색 바 + 제목(최대 2줄) + 흐린 메타 한 줄. 해결은 dim.
function issueRowHTML(i: SpaceIssue): string {
  const [, color] = statusMeta(i.status)
  const gist = i.narrative || i.summary || '' // LLM 요약(한 줄 서사 우선)
  return `
    <div class="issue-row ${i.status === 'resolved' ? 'done' : ''}" data-issue="${esc(i.issue_id)}" style="border-left-color:${color}">
      <div class="issue-row-title">${catBadge(i.category)}${esc(i.title)}</div>
      ${gist ? `<div class="doc-summary">${esc(gist)}</div>` : ''}
      <div class="issue-row-meta"><span>👤 ${esc(issueActor(i))} · ${issueTimeLabel(issueAt(i))}</span>${statusBadge(i.status)}</div>
    </div>`
}

// 이슈 행 클릭 시 뜨는 상세 모달 — 분류·요약·서사 + 진행 상태 타임라인.
// (이슈는 원문 body가 없어 지식 문서처럼 원문 렌더 대신 번역·타임라인을 보여준다.)
function issueModalHTML(i: SpaceIssue): string {
  const steps = (i.timeline ?? [])
    .map(
      (s) =>
        `<li><b>${esc(s.label || s.step)}</b>${s.actor ? ` · 👤 ${esc(s.actor)}` : ''}${
          s.at ? ` · <span class="muted">${esc(kstDateTime(s.at))}</span>` : ''
        }${s.note ? `<div class="muted">${esc(s.note)}</div>` : ''}</li>`,
    )
    .join('')
  return `
    <div class="issue-detail">
      <div class="issue-detail-badges">${catBadge(i.category)}${statusBadge(i.status)}</div>
      ${i.summary ? `<p class="doc-summary">${esc(i.summary)}</p>` : ''}
      ${i.narrative && i.narrative !== i.summary ? `<p class="muted">${esc(i.narrative)}</p>` : ''}
      <div class="muted small">👤 ${esc(issueActor(i))}</div>
      <h4 class="issue-detail-h">진행 상태</h4>
      <ul class="issue-timeline">${steps || '<li class="muted">기록 없음</li>'}</ul>
    </div>`
}

// 지식 문서 클릭 시 모달 — 분류·요약(전문)을 머리에 두고 그 아래 원문 전체를 렌더한다.
// (카드에선 제목·요약을 짧게 클램프하고, 여기서 전문을 보여준다.)
function docModalHTML(d: KnowledgeDoc): string {
  const vis = d.visibility === 'org' ? '조직 공개' : '방 전용'
  return `
    <div class="doc-detail">
      <div class="issue-detail-badges">${catBadge(d.category)}<span class="badge">${vis}</span></div>
      <div class="muted small">👤 ${esc(d.author_agent || '작성자 미상')}</div>
      ${d.summary ? `<p class="doc-summary">${esc(d.summary)}</p>` : ''}
      <h4 class="issue-detail-h">원문</h4>
      <div class="doc-body md">${mdHTML(d.body)}</div>
    </div>`
}

function hubIssuesHTML(data: SpaceView): string {
  const issues = [...data.issues].sort(byRecent)

  // 상태 필터 칩 (개수 포함) — 정렬이 아니라 필터라서 전환해도 배치가 안 바뀐다
  const chipDefs: { id: IssueStatusFilter; label: string }[] = [
    { id: 'all', label: '전체' },
    { id: 'open', label: statusMeta('open')[0] },
    { id: 'knowledge_linked', label: statusMeta('knowledge_linked')[0] },
    { id: 'resolved', label: statusMeta('resolved')[0] },
  ]
  const chips = `<div class="issue-chips">${chipDefs
    .map((c) => {
      const n = c.id === 'all' ? issues.length : issues.filter((i) => i.status === c.id).length
      return `<button class="issue-chip ${issueStatusFilter === c.id ? 'on' : ''}" data-ifilter="${c.id}">${c.label} <b>${n}</b></button>`
    })
    .join('')}</div>`

  // 사람 필터 — 별도 "사람별 보기" 대신 드롭다운 하나로
  const people = [...new Set(issues.map(issueActor))].sort((a, b) => a.localeCompare(b, 'ko'))
  if (issuePersonFilter && !people.includes(issuePersonFilter)) issuePersonFilter = ''
  const personSel = `<div class="issue-person-row"><label class="muted small">사람</label>
    <select id="issue-person" class="issue-person">
      <option value="">전체</option>
      ${people.map((p) => `<option value="${esc(p)}" ${p === issuePersonFilter ? 'selected' : ''}>${esc(p)}</option>`).join('')}
    </select></div>`

  const list = issues
    .filter((i) => issueStatusFilter === 'all' || i.status === issueStatusFilter)
    .filter((i) => !issuePersonFilter || issueActor(i) === issuePersonFilter)

  // 시간 버킷 헤더 — 최신순 리스트를 오늘/어제/이번 주/이전 구간으로 나눈다
  let content = ''
  let bucket = ''
  for (const i of list) {
    const b = timeBucket(issueAt(i))
    if (b !== bucket) {
      bucket = b
      content += `<div class="bucket-head">${b}</div>`
    }
    content += issueRowHTML(i)
  }
  if (!list.length) content = '<p class="muted small">표시할 항목이 없어요</p>'

  return hubSection('이슈 흐름', 'Issue Flow', chips + personSel + content)
}

// ── 지식 문서 필터 (지식 재사용 '책장' · 문서함 공용) ──
// 이슈 흐름과 같은 형태: 태그 칩 + 사람 드롭다운, 정렬은 각 탭 기준 그대로 고정.
// 탭별로 상태를 따로 두는 이유 — 문서함에서 좁힌 조건이 책장까지 바꾸면 위치 감각을 잃는다.
type DocScope = 'reuse' | 'pages'
const docCatFilter: Record<DocScope, string> = { reuse: '', pages: '' } // '' = 전체
const docPersonFilter: Record<DocScope, string> = { reuse: '', pages: '' }

const docAuthor = (d: KnowledgeDoc) => d.author_agent ?? ''
/** 작성자를 모르는 문서(created_by 컬럼 도입 전 초기 페이지)는 목록에서 감춘다 —
 *  '작성자 미상' 줄이 사람 필터의 선택지와 목록을 어지럽히기만 한다. */
const hasAuthor = (d: KnowledgeDoc) => !!d.author_agent?.trim()

/** 칩·드롭다운 마크업. 옵션은 데이터에 실제로 있는 값만, 개수는 다른 축 필터를 반영한다. */
function docFilterHTML(scope: DocScope, docs: KnowledgeDoc[]): string {
  const cats = [...new Set(docs.map((d) => d.category).filter((c): c is string => !!c))].sort((a, b) =>
    (CATEGORY_KO[a] ?? a).localeCompare(CATEGORY_KO[b] ?? b, 'ko'),
  )
  const people = [...new Set(docs.map(docAuthor))].sort((a, b) => a.localeCompare(b, 'ko'))
  // 데이터가 바뀌어 사라진 선택은 '전체'로 되돌린다 (이슈 흐름 선례)
  if (docCatFilter[scope] && !cats.includes(docCatFilter[scope])) docCatFilter[scope] = ''
  if (docPersonFilter[scope] && !people.includes(docPersonFilter[scope])) docPersonFilter[scope] = ''

  const byPerson = docs.filter((d) => !docPersonFilter[scope] || docAuthor(d) === docPersonFilter[scope])
  const chipDefs = [{ id: '', label: '전체' }, ...cats.map((c) => ({ id: c, label: CATEGORY_KO[c] ?? c }))]
  const chips = `<div class="issue-chips">${chipDefs
    .map((c) => {
      const n = c.id === '' ? byPerson.length : byPerson.filter((d) => d.category === c.id).length
      return `<button class="issue-chip ${docCatFilter[scope] === c.id ? 'on' : ''}" data-dfilter="${esc(c.id)}" data-dscope="${scope}">${esc(c.label)} <b>${n}</b></button>`
    })
    .join('')}</div>`
  const personSel = `<div class="issue-person-row"><label class="muted small">사람</label>
    <select class="issue-person" data-dperson="${scope}">
      <option value="">전체</option>
      ${people
        .map((p) => `<option value="${esc(p)}" ${p === docPersonFilter[scope] ? 'selected' : ''}>${esc(p)}</option>`)
        .join('')}
    </select></div>`
  return chips + personSel
}

/** 목록에 필터 적용 — data.knowledge 인덱스(i)를 보존해야 클릭 시 원문 모달이 맞는 문서를 연다. */
function docFilterApply<T extends { d: KnowledgeDoc }>(scope: DocScope, items: T[]): T[] {
  const cat = docCatFilter[scope]
  const person = docPersonFilter[scope]
  return items.filter(
    ({ d }) => hasAuthor(d) && (!cat || d.category === cat) && (!person || docAuthor(d) === person),
  )
}

// ── 지식 재사용 탭 — 재사용 이벤트 피드 + 재사용하면 좋을 지식(책장) ──
function hubReuseHTML(data: SpaceView): string {
  const events = [...(data.reuse_events ?? [])].sort((a, b) => (b.at ?? '').localeCompare(a.at ?? ''))
  const eventItems = events.length
    ? events
        .slice(0, 30)
        .map((e) => {
          const dir = e.source_space === data.space_id ? '이 방의 지식이 재사용됨' : '타팀 지식을 재사용'
          const at = kstTime(e.at)
          return `
        <div class="feed-card" data-reuse-doc="${esc(e.doc_id ?? '')}">
          <div class="fc-title">🔄 ${dir}${at ? ` <span class="tl-at">${at}</span>` : ''}</div>
          <div class="doc-summary">${esc(e.summary)}</div>
        </div>`
        })
        .join('')
    : '<p class="muted small">표시할 항목이 없어요</p>'

  // 책장 = "재사용하면 좋을 지식" — 재사용 많은 순, 조직 공개 우선. 클릭 시 원문 모달.
  // 상한(20)은 필터를 적용한 뒤에 — 좁혀 놓고도 20개 밖의 문서가 안 보이면 필터가 무의미하다.
  const docs = docFilterApply(
    'reuse',
    data.knowledge.map((d, i) => ({ d, i })),
  )
    .sort(
      (a, b) =>
        (a.d.visibility === 'org' ? 0 : 1) - (b.d.visibility === 'org' ? 0 : 1) ||
        b.d.reuse_count - a.d.reuse_count,
    )
    .slice(0, 20)
  const docItems = docs.length
    ? docs
        .map(
          ({ d, i }) => `
        <div class="doc-item" data-doc="${i}" data-docid="${esc(d.doc_id)}">
          <b>${catBadge(d.category)}${esc(d.title)}</b>
          <div class="muted">👤 ${esc(d.author_agent || '작성자 미상')}${d.visibility === 'org' ? ' · 조직 공개' : ''} · 재사용 ${d.reuse_count ?? 0}</div>
          <div class="doc-summary">${esc(d.summary)}</div>
        </div>`,
        )
        .join('')
    : '<div class="muted small">등록된 지식이 없습니다.</div>'

  return (
    hubSection('지식 재사용', 'Knowledge Reuse', eventItems) +
    hubSection('책장 — 재사용하면 좋을 지식', 'Bookshelf', docFilterHTML('reuse', data.knowledge.filter(hasAuthor)) + docItems)
  )
}

// 'page_12' → 12. 허브 지식 문서엔 타임스탬프가 없어 id 순서를 의사 시간으로 쓴다
// (백엔드 collector._id_seq와 동일 관례).
function docSeq(docId: string): number {
  const m = /(\d+)$/.exec(docId ?? '')
  return m ? Number(m[1]) : 0
}

// ── 문서함 탭 — create_page로 남긴 모든 지식 문서를 최신순으로 전부 나열 ──
// '지식 재사용' 탭의 책장은 재사용 많은 순 상위 20개만 추리지만, 여기는 필터 없이 전체를
// 훑어보는 용도 — 허브에 남긴 작업 요약·새 사실이 어딘가엔 반드시 보이게 한다.
function hubPagesHTML(data: SpaceView): string {
  const docs = docFilterApply(
    'pages',
    data.knowledge.map((d, i) => ({ d, i })),
  ).sort((a, b) => docSeq(b.d.doc_id) - docSeq(a.d.doc_id))
  const items = docs.length
    ? docs
        .map(
          ({ d, i }) => `
        <div class="doc-item" data-doc="${i}" data-docid="${esc(d.doc_id)}">
          <b>${catBadge(d.category)}${esc(d.title)}</b>
          <div class="muted">👤 ${esc(d.author_agent || '작성자 미상')} · ${d.visibility === 'org' ? '조직 공개' : '방 전용'}</div>
          <div class="doc-summary">${esc(d.summary)}</div>
        </div>`,
        )
        .join('')
    : '<p class="muted small">표시할 항목이 없어요</p>'
  return hubSection('문서함', 'All Pages', docFilterHTML('pages', data.knowledge.filter(hasAuthor)) + items)
}

type ActivityTab = 'online' | 'offline'
let activityTab: ActivityTab = 'online'

function agentRowHTML(a: SpaceAgent): string {
  const at = kstTime(a.last_active_at)
  const status = a.status_line || (a.status === 'working' ? '활동 중' : at ? `마지막 활동 ${at}` : '자리 비움')
  return `
    <div class="agent-row ${a.status === 'working' ? '' : 'off'}">
      <span class="agent-dot ${a.status === 'working' ? 'on' : ''}"></span>
      <span class="agent-name">${esc(a.name)}</span>
      <span class="agent-status">${esc(status)}</span>
    </div>`
}

/** 활동 기록이 하나도 없는 멤버 — 페이지·이슈를 한 번도 쓰지 않아 백엔드가 last_write를
 *  못 찾은 계정(collector `_agent`). 테스트·프로브용 등록만 남은 계정이라 목록에서 감춘다. */
function hasSentAnything(a: SpaceAgent): boolean {
  return a.last_active_at !== null || !!a.recent_activity
}

function hubActivityHTML(data: SpaceView): string {
  const active = data.agents.filter(hasSentAnything)
  const online = active.filter((a) => a.status === 'working')
  const offline = active.filter((a) => a.status !== 'working')
  const list = activityTab === 'online' ? online : offline
  const items = list.length ? list.map(agentRowHTML).join('') : '<p class="muted small">표시할 항목이 없어요</p>'
  return `
    <section class="feed activity-feed">
      <div class="feed-head"><h3>팀 활동</h3></div>
      <div class="activity-subtabs">
        <button class="sub-tab ${activityTab === 'online' ? 'on' : ''}" data-atab="online">온라인 ${online.length}</button>
        <button class="sub-tab ${activityTab === 'offline' ? 'on' : ''}" data-atab="offline">오프라인 ${offline.length}</button>
      </div>
      <div class="activity-list">${items}</div>
    </section>`
}

function hubSection(title: string, sub: string, items: string): string {
  return `
    <section class="feed">
      <div class="feed-head"><h3>${esc(title)}</h3><span class="feed-sub">${esc(sub)}</span></div>
      ${items}
    </section>`
}

// 사이드바 접힘 상태 — 다음 입장 때도 유지되게 localStorage에 기억.
// 사설 모드·iframe 등에서 localStorage 접근이 막혀도 앱이 죽지 않게 감싼다.
let hubCollapsed = false
try {
  hubCollapsed = localStorage.getItem('a-lens.hub.collapsed') === '1'
} catch (e) {
  console.warn('localStorage 읽기 실패 — 접힘 상태 기본값 사용', e)
}

/** 방 화면에서만 호출 — 접힘 여부에 따라 사이드바/열기버튼 표시를 정하고 씬 폭을 재조정한다. */
function applyHubCollapsed(inLife: boolean) {
  hub.hidden = !inLife || hubCollapsed
  hubOpen.hidden = !inLife || !hubCollapsed
  fitScene()
}

function toggleHub(collapsed: boolean) {
  hubCollapsed = collapsed
  try {
    localStorage.setItem('a-lens.hub.collapsed', collapsed ? '1' : '0')
  } catch (e) {
    console.warn('localStorage 쓰기 실패 — 접힘 상태 저장 생략', e)
  }
  applyHubCollapsed(true)
  if (!panel.hidden) positionPanel() // 열린 상세 패널도 새 위치로 따라오게
}
$('hub-collapse').addEventListener('click', () => toggleHub(true))
hubOpen.addEventListener('click', () => toggleHub(false))

// ── Hub 너비 드래그 리사이즈 (localStorage 유지) ──
const HUB_MIN = 300
const HUB_MAX = 720
let hubWidth = 400
try {
  const w = Number(localStorage.getItem('a-lens.hub.width'))
  if (w) hubWidth = Math.min(HUB_MAX, Math.max(HUB_MIN, w))
} catch (e) {
  console.warn('localStorage 읽기 실패 — Hub 너비 기본값 사용', e)
}
function applyHubWidth() {
  document.documentElement.style.setProperty('--hub-w', `${hubWidth}px`)
  fitScene()
}
applyHubWidth()
const hubResize = $('hub-resize')
hubResize.addEventListener('pointerdown', (e: PointerEvent) => {
  e.preventDefault()
  hubResize.classList.add('dragging')
  const onMove = (ev: PointerEvent) => {
    // Hub는 오른쪽에 고정 — 왼쪽 가장자리를 끌면 너비 = 화면오른쪽 − 커서X
    hubWidth = Math.min(HUB_MAX, Math.max(HUB_MIN, Math.round(window.innerWidth - ev.clientX)))
    applyHubWidth()
  }
  const onUp = () => {
    hubResize.classList.remove('dragging')
    try {
      localStorage.setItem('a-lens.hub.width', String(hubWidth))
    } catch {
      /* 저장 실패 무시 */
    }
    window.removeEventListener('pointermove', onMove)
    window.removeEventListener('pointerup', onUp)
  }
  window.addEventListener('pointermove', onMove)
  window.addEventListener('pointerup', onUp)
})

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

  // 위 2/3: 선택 탭 내용. 아래 1/3: 팀 활동 상시 (온라인/오프라인 서브탭).
  hubBody.innerHTML =
    hubTab === 'issues' ? hubIssuesHTML(data) : hubTab === 'reuse' ? hubReuseHTML(data) : hubPagesHTML(data)
  // 이슈 흐름 필터 — 상태 칩 + 사람 드롭다운
  hubBody.querySelectorAll<HTMLElement>('[data-ifilter]').forEach((btn) => {
    btn.addEventListener('click', () => {
      issueStatusFilter = btn.dataset.ifilter as IssueStatusFilter
      renderHub(data)
    })
  })
  hubBody.querySelector<HTMLSelectElement>('#issue-person')?.addEventListener('change', (e) => {
    issuePersonFilter = (e.target as HTMLSelectElement).value
    renderHub(data)
  })
  // 지식 문서 필터 — 태그 칩 + 사람 드롭다운 (책장·문서함 공용, 탭별 상태)
  hubBody.querySelectorAll<HTMLElement>('[data-dfilter]').forEach((btn) => {
    btn.addEventListener('click', () => {
      docCatFilter[btn.dataset.dscope as DocScope] = btn.dataset.dfilter ?? ''
      renderHub(data)
    })
  })
  hubBody.querySelectorAll<HTMLSelectElement>('[data-dperson]').forEach((sel) => {
    sel.addEventListener('change', () => {
      docPersonFilter[sel.dataset.dperson as DocScope] = sel.value
      renderHub(data)
    })
  })
  // 지식 재사용 탭의 책장 문서 → 원문 모달
  hubBody.querySelectorAll<HTMLElement>('.doc-item').forEach((el) => {
    el.addEventListener('click', () => {
      const doc = data.knowledge[Number(el.dataset.doc)]
      if (doc) showModal(doc.title, docModalHTML(doc))
    })
  })
  // 이슈 흐름의 행 클릭 → 이슈 상세 모달 (분류·요약·타임라인)
  hubBody.querySelectorAll<HTMLElement>('.issue-row').forEach((el) => {
    el.addEventListener('click', () => {
      const iss = data.issues.find((x) => x.issue_id === el.dataset.issue)
      if (iss) showModal(iss.title, issueModalHTML(iss))
    })
  })
  renderHubActivity(data)
  applyHubCollapsed(true)
}

/** 사이드바에서 selector에 걸리는 카드들을 금색 테두리로 강조하고 첫 항목으로 스크롤. */
function flashRelated(selector: string) {
  const els = hubBody.querySelectorAll<HTMLElement>(selector)
  els.forEach((el) => el.classList.add('hl-related'))
  els[0]?.scrollIntoView({ block: 'center', behavior: 'smooth' })
}

function renderHubActivity(data: SpaceView) {
  hubActivity.innerHTML = hubActivityHTML(data)
  hubActivity.querySelectorAll<HTMLElement>('.sub-tab').forEach((btn) => {
    btn.addEventListener('click', () => {
      activityTab = btn.dataset.atab as ActivityTab
      renderHubActivity(data) // 활동 영역만 다시 — 위 탭 내용·스크롤 유지
    })
  })
}

async function renderLife(spaceId: string) {
  const config = getLife(spaceId)
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
    <b id="life-title">${esc(config.space_name)}</b>
    <span class="demo-badge" id="life-demo" hidden>FAKE</span>
    <span class="muted" id="life-visits"></span>
    <span class="spacer"></span>
    <button class="ghost-btn sm" id="btn-edit-life">꾸미기</button>`
  $('btn-back').addEventListener('click', () => (location.hash = ''))
  $('btn-edit-life').addEventListener('click', async () => {
    const floors = await getFloors()
    void openBuilder({
      floors,
      initial: getLife(spaceId),
      onSaved: (c) => {
        saveLife(c)
        void renderLife(spaceId)
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
  if (location.hash !== `#life/${spaceId}` && location.hash !== `#life/${encodeURIComponent(spaceId)}`) {
    return
  }

  const title = $('life-title')
  const floors = floorsCache
  const liveName = floors?.find((f) => f.space_id === spaceId)?.name
  if (liveName) {
    title.textContent = liveName
    config.space_name = liveName // 칠판 위 간판에도 최신 이름 반영
  }
  $('life-visits').textContent = data.visits ? `방문 TODAY ${data.visits.today} · TOTAL ${data.visits.total}` : ''
  $('life-demo').hidden = !data.demo // 더미(fake) 스페이스 구분 배지

  if (currentScene) {
    currentScene.destroy({ children: true })
    currentScene = null
  }
  app.stage.removeChildren()
  // 방 안 캐릭터도 '팀 활동' 목록과 같은 기준 — 허브에 아무것도 안 보낸 계정(테스트·프로브
  // 등록만 남은 멤버)은 책상을 차지하지 않는다(hasSentAnything).
  // 그 위에 과밀 방지 상한: 온라인 우선·최근 활동순 상위 N명만. 여기서 잘린 사람은 목록에 남는다.
  const SCENE_MAX_AGENTS = 24
  const sceneAgents = data.agents
    .filter(hasSentAnything)
    .sort(
      (a, b) =>
        (a.status === 'working' ? 0 : 1) - (b.status === 'working' ? 0 : 1) ||
        (b.last_active_at ?? '').localeCompare(a.last_active_at ?? ''),
    )
    .slice(0, SCENE_MAX_AGENTS)
  currentScene = buildLifeScene(
    config,
    {
      agents: sceneAgents,
      issues: data.issues,
      // 책장 배지 건수는 '문서함'이 실제로 보여주는 수와 같아야 한다 — 저자 미상은 뺀 수.
      knowledgeCount: data.knowledge.filter(hasAuthor).length,
      highlight: data.highlight?.text,
    },
    {
      onAgentTap: (agent) => showAgentPanel(agent),
      // 칠판 클릭 → 하이라이트와 관련된 이슈/재사용 항목을 해당 탭에서 강조 (2026-07-19).
      onBoardTap: () => {
        if (hubCollapsed) toggleHub(false)
        const h = data.highlight
        if (h?.kind === 'reuse' && h.doc_id) {
          hubTab = 'reuse'
          renderHub(data)
          flashRelated(`[data-reuse-doc="${h.doc_id}"], [data-docid="${h.doc_id}"]`)
        } else if (h?.kind === 'issue' && h.issue_id) {
          hubTab = 'issues'
          // 필터에 가려 강조 대상이 안 보이는 일이 없게 필터를 초기화하고 연다
          issueStatusFilter = 'all'
          issuePersonFilter = ''
          renderHub(data)
          flashRelated(`[data-issue="${h.issue_id}"]`)
        } else {
          hubTab = 'issues' // 하이라이트 없으면(허브 실데이터 등) 이슈 흐름만 연다
          renderHub(data)
        }
      },
      // 책장 클릭 → Hub '문서함' 탭 열기 (접혀 있으면 펼침) — 책장은 지식 문서가 쌓이는
      // 곳이라는 은유이므로, 재사용 이벤트 피드보다 전체 문서 목록을 보여주는 쪽이 맞다.
      onShelfTap: () => {
        if (hubCollapsed) toggleHub(false)
        hubTab = 'pages'
        renderHub(data)
      },
      // 창문 클릭 → 창밖(방 프리셋) 풍경을 두고 주고받는 잡담 말풍선
      onWindowTap: (at) => void playRoomChat('window', at, resolveLifeId(config.life), config.space_name ?? ''),
      // 정수기 클릭 → 물 권하며 쉬어가라는 말풍선
      onWaterTap: (at) => void playRoomChat('water', at, resolveLifeId(config.life), config.space_name ?? ''),
      // 책장 배지 클릭 → 쌓인 문서량 자랑 + 문서함 안내 (탭은 열지 않는다 — 말풍선이 안내한다)
      onShelfBadgeTap: (at) => playShelfBadgeChat(at, data.knowledge.filter(hasAuthor).length),
    },
  )
  app.stage.addChild(currentScene)
  // 창문·정수기 대사 미리 받아두기 — 첫 클릭이 4초 기다리지 않게 (실패해도 무해)
  prefetchRoomChat('window', resolveLifeId(config.life), config.space_name ?? '')
  prefetchRoomChat('water', resolveLifeId(config.life), config.space_name ?? '')

  // 오른쪽 Collaboration Hub — 내용 채우고, 접힘 상태에 맞춰 표시 + 씬 폭 재조정(fitScene).
  hubTab = 'issues'
  renderHub(data)
}

// ── 해시 라우팅 ──
function route() {
  const m = location.hash.match(/^#life\/(.+)$/)
  if (m) void renderLife(decodeURIComponent(m[1]))
  else void renderHome()
}
window.addEventListener('hashchange', route)
// 스프라이트 킷 + 서버의 공유 방 목록을 먼저 로드하고 첫 라우팅 — 킷 없으면 Graphics 폴백
void loadKit()
  .then(loadRooms)
  .then(route)
