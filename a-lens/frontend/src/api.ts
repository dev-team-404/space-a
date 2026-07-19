// 백엔드 뷰모델 API — 번역은 서버 몫, 프론트는 뷰모델만 받는다 (ADR 0003).

export type LobbyFloor = {
  space_id: string
  name: string
  floor: number | null
  activity: number
  stats: { knowledge?: number; reuse?: number; resolved?: number }
  highlight: string | null
}

export type LobbyView = {
  floors: LobbyFloor[]
  totals: Record<string, number>
  tokens_saved_est: number | null
  highlight: { summary?: string } | null
}

export type SpaceAgent = {
  agent_id: string
  name: string
  role: string
  owner: string
  status: 'working' | 'idle' | string
  status_line: string
  last_active_at: string | null
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

export type SpaceView = {
  space_id: string
  viewer_tier: string
  agents: SpaceAgent[]
  issues: SpaceIssue[]
  knowledge: KnowledgeDoc[]
  visits: { today: number; total: number } | null
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
