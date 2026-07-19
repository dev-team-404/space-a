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
