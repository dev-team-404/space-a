// 백엔드 뷰모델 API — 번역은 서버 몫, 프론트는 뷰모델만 받는다 (ADR 0003).

export type LobbyFloor = {
  space_id: string
  name: string
  floor: number | null
  activity: number
  stats: { knowledge?: number; reuse?: number; resolved?: number }
  highlight: string | null
  demo?: boolean // 더미(fake) 스페이스 — 화면에서 FAKE 배지로 구분
}

export type LobbyView = {
  floors: LobbyFloor[]
  totals: Record<string, number>
  tokens_saved_est: number | null
  highlight: { summary?: string } | null
}

export type RecentActivity = {
  brief: string // 말풍선용 짧은 문장
  detail: string // 상세 패널용 풀어쓴 설명
}

export type SpaceAgent = {
  agent_id: string
  name: string
  role: string
  owner: string
  status: 'working' | 'idle' | string
  status_line: string
  last_active_at: string | null
  recent_activity?: RecentActivity | null
}

export type IssueStep = {
  step: string
  label: string
  actor: string
  at: string | null
  note: string
}

export type SpaceIssue = {
  issue_id: string
  title: string
  status: 'open' | 'knowledge_linked' | 'resolved' | string
  opened_by: string
  timeline: IssueStep[]
  category?: string // LLM 분류
  summary?: string // LLM 요약
  narrative?: string // LLM 한 줄 서사
}

export type KnowledgeDoc = {
  doc_id: string
  title: string
  author_agent: string
  visibility: string
  summary: string
  body: string
  cited_by: string[]
  reuse_count: number
  category?: string // LLM 분류 (문제해결/설계·스펙/…)
  narrative?: string // LLM 한 줄 서사
}

export type SpaceHighlight = {
  text: string // 칠판에 분필로 적히는 한 줄
  kind?: 'reuse' | 'issue' | string
  doc_id?: string // kind=reuse — 재사용된 지식 문서
  issue_id?: string // kind=issue — 해결된 이슈
}

export type ReuseEvent = {
  doc_id?: string
  source_space?: string
  consumer_space?: string
  at?: string | null
  summary: string
  demo?: boolean
}

export type SpaceView = {
  space_id: string
  viewer_tier: string
  agents: SpaceAgent[]
  issues: SpaceIssue[]
  knowledge: KnowledgeDoc[]
  visits: { today: number; total: number } | null
  reuse_events?: ReuseEvent[] // 이 방이 원천/소비자인 재사용 이벤트 (Hub '지식 재사용' 탭)
  highlight?: SpaceHighlight | null // 오늘의 하이라이트 — 방 칠판 표시 + 클릭 시 관련 항목 연동
  demo?: boolean // 더미(fake) 스페이스 — 화면에서 FAKE 배지로 구분
}

async function getJson<T>(url: string): Promise<T> {
  const res = await fetch(url)
  if (!res.ok) throw new Error(`GET ${url} → ${res.status}`)
  return res.json() as Promise<T>
}

export function fetchLobby(): Promise<LobbyView> {
  return getJson<LobbyView>('/api/lobby')
}

export function fetchSpace(spaceId: string, tier = 'member'): Promise<SpaceView> {
  return getJson<SpaceView>(`/api/spaces/${encodeURIComponent(spaceId)}?tier=${tier}`)
}

// 방 오브젝트 클릭(창문·정수기) → 그 자리에 어울리는 잡담 2~3줄. LLM 없으면 서버가 폴백 문구.
export type RoomChatSpot = 'window' | 'water'
export type RoomChat = { lines: string[]; source: string }

export function fetchRoomChat(spot: RoomChatSpot, view: string, spaceName = ''): Promise<RoomChat> {
  return getJson<RoomChat>(
    `/api/room-chat?spot=${spot}&view=${encodeURIComponent(view)}&space_name=${encodeURIComponent(spaceName)}`,
  )
}

// ── 설정 (설정 창) — a-hub 연결 · LLM API 런타임 구성 ──
// 비밀값은 서버가 값 대신 *_set 불리언으로만 내려준다 (마스킹).
export type Settings = {
  source: string
  work_url: string
  work_token_set: boolean
  work_api_key_set: boolean
  presence_window: number
  cache_ttl: number
  llm_url: string
  llm_model: string
  llm_key_set: boolean
  summary_style: string // brief | normal | detailed
  db_path: string
}

export type ConnResult = { ok: boolean; status?: number; error?: string }
export type ConnTest = { hub: ConnResult; llm: ConnResult }

export function fetchSettings(): Promise<Settings> {
  return getJson<Settings>('/api/settings')
}

async function postJson<T>(url: string, body: unknown): Promise<T> {
  const res = await fetch(url, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body ?? {}),
  })
  if (!res.ok) throw new Error(`POST ${url} → ${res.status}`)
  return res.json() as Promise<T>
}

export function saveSettings(patch: Record<string, unknown>): Promise<Settings> {
  return postJson<Settings>('/api/settings', patch)
}

// 저장 전 폼 값으로도 검사할 수 있게 patch를 함께 보낸다 (빈 비밀값은 저장된 값으로 폴백).
export function testConnections(patch: Record<string, unknown> = {}): Promise<ConnTest> {
  return postJson<ConnTest>('/api/settings/test', patch)
}
