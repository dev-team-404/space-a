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
  // 공통 신원(2026-07-29) — Life 서버에서 온 사람 정보. 매칭 실패면 없다.
  life_agent_id?: string
  hub_name?: string // Life 이름으로 덮이기 전의 허브 표시 이름
  merged_ids?: string[] // 같은 사람의 다른 허브 계정 id — 이 줄이 흡수했다
  mascot_url?: string // 백엔드 프록시 URL (/api/life-mascot/{life_agent_id})
  via?: string // 'life' = Hub 계정과 아직 못 이은 Life 사람 (활동 정보 없음)
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
  project?: string | null // a-mate가 실어 보낸 저장소 이름 (마커 없는 옛 기록은 null)
}

export type KnowledgeDoc = {
  doc_id: string
  title: string
  author_agent: string
  author_agent_id?: string // 작성자 원본 agent_id — 사람별 작업 기록을 묶는 키
  visibility: string
  summary: string
  body: string
  cited_by: string[]
  reuse_count: number
  category?: string // LLM 분류 (문제해결/설계·스펙/…)
  narrative?: string // LLM 한 줄 서사
  project?: string | null // a-mate가 실어 보낸 저장소 이름 (마커 없는 옛 문서는 null)
}

export type SpaceHighlight = {
  text: string // 칠판에 분필로 적히는 한 줄
  kind?: 'reuse' | 'knowledge' | 'issue' | string
  doc_id?: string // kind=reuse|knowledge — 재사용된/새로 등록된 지식 문서
  issue_id?: string // kind=issue — 이슈
}

export type ReuseEvent = {
  doc_id?: string
  source_space?: string
  consumer_space?: string
  at?: string | null
  summary: string
  demo?: boolean
}

// ── 협업 지도 (스펙: docs/design/a-lens/specs/2026-07-31-collab-graph.md) ──
// 사실(reuse·handoff)과 추정(topic)은 **엣지 종류로 분리해서** 온다. 화면에서 합치지 않는다.
export type CollabNode = {
  id: string
  name: string
  status: string
  mascot_url?: string | null
  docs: number
  issues: number
  external: boolean // 이 방 밖 사람 — 우리 지식을 가져간 쪽
}

export type CollabEdge = {
  type: 'reuse' | 'handoff' | 'topic'
  source: string
  target: string
  weight: number
  keywords?: string[] // topic — 두 사람이 공유한 주제어
  // topic — 근거 문서쌍. docs=[source쪽, target쪽], keywords=그 쌍이 공유한 말
  doc_pairs?: { docs: string[]; keywords: string[] }[]
  doc_ids?: string[] // reuse — 재사용된 문서
  issue_ids?: string[] // handoff — 넘겨받아 해결한 이슈
}

export type CollabStats = {
  reuse: number
  handoff: number
  topic: number
  issues: number
  self_resolved: number // 자문자답(연 사람이 곧 해결한 사람) 건수
  docs: number
  unlinked_docs: number // 작성자를 사람으로 못 이은 문서
  quiet_people: number // 이 방에서 활동 기록이 없어 지도에서 뺀 계정
  truncated_docs: number
  dropped_edges: number
  keywords: number
}

// 프로젝트 축 — a-mate가 세션 작업 디렉터리(= git 저장소 이름)를 문서 제목에 실어 보낸 것.
// **추정이 아니라 사실**이라 주제 겹침과 성격이 다르다. 마커가 없는 옛 문서는 unknown_docs로만 센다.
export type CollabProject = {
  id: string // 저장소 이름 슬러그 (예: space-a)
  docs: number
  people: { id: string; docs: number }[]
  // 이 프로젝트만의 지도 — 방 지도와 같은 잣대로 만든 부분집합이다
  graph?: { nodes: CollabNode[]; edges: CollabEdge[]; stats: CollabStats }
}

export type CollabProjects = {
  nodes: CollabProject[]
  unknown_docs: number // 마커가 없어 어느 프로젝트인지 모르는 문서 (소급 적용 없음)
}

export type CollabGraph = {
  nodes: CollabNode[]
  edges: CollabEdge[]
  stats: CollabStats
  projects?: CollabProjects
  // 항목(doc_id·issue_id) → 그 항목을 구별해주는 말. 사이드바 네 탭이 금색으로 짚는다.
  terms?: Record<string, string[]>
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
  collab?: CollabGraph | null // 협업 지도 (계산 실패·로딩 전이면 null)
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
  // a-hub(life) 연결 — 사람 이름·마스코트 (공통 신원). 비우면 Life 연동 off.
  life_url?: string
  life_token_set?: boolean
  life_api_key_set?: boolean
  life_alias?: string
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
