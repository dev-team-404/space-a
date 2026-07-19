import { invoke } from '@tauri-apps/api/core';
import { emit, listen, type UnlistenFn } from '@tauri-apps/api/event';
import type { RobotSpec } from './robot/render';

export interface Summary {
  date: string;
  user_name: string;
  session_count: number;
  tok_input: number;
  tok_output: number;
  tok_cache_read: number;
  tok_cache_create: number;
  total_sessions: number;
  est_tokens_saved_total: number;
  last_scan: string | null;
}

export interface Finding {
  rule_id: string;
  severity: 'info' | 'suggest' | 'warn';
  scope_host: string | null;
  scope_project: string | null;
  scope_kind: string;
  scope_ref: string;
  evidence: unknown;
  est_tokens_saved: number;
  prescription: { kind: string; payload: unknown } | null;
  dedup_key: string;
  last_seen: string | null;
  occurrences: number;
  status: string;
}

export interface CoachFinding extends Finding {
  status: 'new' | 'resolved' | 'dismissed';
  detail: string;
  suggested_action: string;
  fix_command: string | null;
  session: { project_id: string; first_ts: string | null } | null;
}

export interface TranscriptEntry {
  ts: string | null;
  role: 'user' | 'assistant';
  text: string;
  tools: string[];
  model: string | null;
}

export interface DayStat {
  date: string;
  tok_input: number;
  tok_output: number;
  session_count: number;
}

export interface ModelMixEntry {
  tier: string;
  tokens: number;
}

export type ModelMixPeriod = 'today' | 'week' | 'month' | 'all';

export interface SessionCtxItem {
  session_id: string;
  project_id: string;
  first_ts: string | null;
  cwd: string | null;
  first_prompt: string | null;
}

export interface ChatMessage {
  role: 'user' | 'assistant';
  content: string;
}

export interface ChatStatus {
  configured: boolean;
  model: string | null;
}

/** 큐레이션 콘텐츠 — 역량 사다리 팁(dimension 있음) 또는 피드 뉴스(dimension null). */
export interface ContentItem {
  id: string;
  kind: 'tip' | 'news';
  dimension: string | null;
  title: string;
  body: string;
  source_url: string | null;
  trigger_tags: string[];
  score: number;
  status: 'new' | 'shown' | 'dismissed';
  /** "당신 로그: …" — 사용자 실측 데이터로 접지한 근거 줄(없을 수 있음). */
  personal?: string | null;
}

export const getSummary = () => invoke<Summary>('get_summary');
export const listFindings = (includeHidden = false) =>
  invoke<CoachFinding[]>('list_findings', { includeHidden });
export const setFindingStatus = (dedupKey: string, status: 'new' | 'resolved' | 'dismissed') =>
  invoke<void>('set_finding_status', { dedupKey, status });
export const getWeekSummary = () => invoke<DayStat[]>('get_week_summary');
export const getModelMix = (period: ModelMixPeriod = 'today') =>
  invoke<ModelMixEntry[]>('get_model_mix', { period });
export const getTodayOccasions = () => invoke<string[]>('get_today_occasions');
export const getSessionTranscript = (sessionId: string) =>
  invoke<TranscriptEntry[]>('get_session_transcript', { sessionId });
export const runScanNow = () => invoke<void>('run_scan_now');
export const getMascotSeed = () => invoke<RobotSpec>('get_mascot_seed');
export const openChatTab = (tab: string, target?: string) =>
  invoke<void>('open_chat_tab', { tab, target });
export const getSettings = () => invoke<Record<string, string>>('get_settings');
export const setSetting = (key: string, value: string) => invoke<void>('set_setting', { key, value });
export const listDiaryDates = () => invoke<string[]>('list_diary_dates');
export const getDiary = (date: string) => invoke<string | null>('get_diary', { date });
export const getDailyLine = () => invoke<string | null>('get_daily_line');
export const getChatterPool = () => invoke<string[]>('get_chatter_pool');
export const sessionsCtx = (ids: string[]) =>
  invoke<SessionCtxItem[]>('sessions_ctx', { ids });
export const chatStatus = () => invoke<ChatStatus>('chat_status');
export const chatSend = (messages: ChatMessage[]) => invoke<string>('chat_send', { messages });
export const listContent = (includeHidden = false) =>
  invoke<ContentItem[]>('list_content', { includeHidden });
export const setContentStatus = (id: string, status: 'new' | 'shown' | 'dismissed') =>
  invoke<void>('set_content_status', { id, status });
/** (2) LLM 코칭 — 팁+개인 근거를 엔진에 넘겨 맞춤 한 줄 생성. 엔진 미설정이면 reject. */
export const coachTip = (item: ContentItem) =>
  invoke<string>('coach_tip', { title: item.title, body: item.body, personal: item.personal ?? null });

// 마스코트가 pull한 occasion을 chat 창 알림 로그용으로 재방송 (pull 단일화 — 플랜 Task 2 Step 5)
export const emitOccasionToday = (labels: string[]) => emit('occasion:today', labels);

// --- 방 방문 (docs/design/room-visit.md) ---

export interface HubSettings {
  url: string;
  user: string;
  connected: boolean;
  room_id: string;
}
export interface RoomOccupant {
  agent_id: string;
  name: string;
  cell: [number, number];
  is_owner: boolean;
  mascot_seed: string;
}
export interface RoomState {
  room_id: string;
  owner_name: string;
  owner_mascot_seed: string; // 주인이 방을 비워도 프로필 로봇을 그릴 수 있게 서버가 항상 준다
  grid: { w: number; h: number };
  design: { wallpaper: string; floor: string; objects: RoomObject[] };
  occupants: RoomOccupant[];
}
export interface RoomObject {
  asset_id: string;
  category: string;
  cell: [number, number];
  size: [number, number];
  footprint?: [number, number][];
  rotation: 0 | 90 | 180 | 270;
  wall?: 'north' | 'west' | null;
}
export interface RoomMe {
  agent_id: string;
  name: string;
  my_room_id: string;
  room_id: string;
  cell: [number, number];
}
export interface RoomListEntry {
  room_id: string;
  owner_name: string;
  occupants: number;
}
export interface RoomCapabilities { room_protocol: number; grid: { w: number; h: number }; floor_min_y: number; footprint_mask: boolean; wall_objects: boolean }

export const hubSettingsGet = () => invoke<HubSettings>('hub_settings_get');
export const hubConnect = (url: string, user: string) =>
  invoke<HubSettings>('hub_connect', { url, user });
export const roomView = () => invoke<{ me: RoomMe; room: RoomState }>('room_view');
export const roomCapabilities = () => invoke<RoomCapabilities>('room_capabilities');
export const roomsList = () => invoke<{ rooms: RoomListEntry[] }>('rooms_list');
export const roomGoto = (roomId: string) => invoke<RoomMe>('room_goto', { roomId });
export const roomMoveCell = (x: number, y: number) => invoke<RoomMe>('room_move_cell', { x, y });
export const roomSaveDesign = (roomId: string, design: RoomState['design']) =>
  invoke<RoomState>('room_save_design', { roomId, design });
export const robotSpecForSeed = (seed: string) => invoke<RobotSpec>('robot_spec_for_seed', { seed });
export const openSettingsWindow = () => invoke<void>('open_settings_window');
// 마스코트 창 확장/복귀 — 위치+크기를 네이티브에서 한 번에 적용 (중간 프레임 깜빡임 방지)
export const mascotSetExpanded = (expanded: boolean) =>
  invoke<void>('mascot_set_expanded', { expanded });

export interface ScanProgress {
  done: number;
  total: number;
}

/** chat:goto-tab payload — target은 탭 문맥으로 해석(coach→dedup_key, diary→YYYY-MM-DD) */
export interface GotoTabPayload {
  tab: string;
  target?: string;
}

export const onScanProgress = (cb: (p: ScanProgress) => void): Promise<UnlistenFn> =>
  listen<ScanProgress>('scan:progress', (e) => cb(e.payload));

export const onScanDone = (cb: (ts: string) => void): Promise<UnlistenFn> =>
  listen<string>('scan:done', (e) => cb(e.payload));
export const onNewFindings = (cb: (rows: Finding[]) => void): Promise<UnlistenFn> =>
  listen<Finding[]>('coach:finding', (e) => cb(e.payload));
export const onDiaryReady = (cb: (date: string) => void): Promise<UnlistenFn> =>
  listen<string>('diary:ready', (e) => cb(e.payload));
export const onDailyLine = (cb: (text: string) => void): Promise<UnlistenFn> =>
  listen<string>('daily-line:ready', (e) => cb(e.payload));
export const onOccasionToday = (cb: (labels: string[]) => void): Promise<UnlistenFn> =>
  listen<string[]>('occasion:today', (e) => cb(e.payload));
export const onGotoTab = (cb: (p: GotoTabPayload) => void): Promise<UnlistenFn> =>
  listen<GotoTabPayload>('chat:goto-tab', (e) => cb(e.payload));
export const onSettingsChanged = (cb: () => void): Promise<UnlistenFn> =>
  listen('settings:changed', () => cb());
export const onContentReady = (cb: (rows: ContentItem[]) => void): Promise<UnlistenFn> =>
  listen<ContentItem[]>('content:ready', (e) => cb(e.payload));

/** AI 스프라이트(캐시) base64 — 없으면 null (절차 생성 폴백). */
export async function getSprite(): Promise<string | null> {
  try { return await invoke<string | null>('get_sprite'); } catch { return null; }
}

export interface ProfileRung {
  key: string;
  label: string;
  ladder_index: number;
  mastery: 'not_started' | 'in_progress' | 'mastered';
  evidence: string;
  learn_hint: string;
  is_frontier: boolean;
}
export interface ProfileView {
  rungs: ProfileRung[];
  frontier_key: string | null;
  total_events: number;
}

/** AX 역량 사다리 — 각 축의 숙련도 + 지금 배울 것(frontier). */
export async function getProfile(): Promise<ProfileView> {
  return invoke<ProfileView>('get_profile');
}

export interface SkillDraft {
  markdown: string;
  slug: string;
  llm_generated: boolean;
  session_count: number;
}

/** R6 반복 지시(host + 대표 프롬프트) → SKILL.md 초안 생성. */
export async function generateSkillDraft(host: string, representative: string): Promise<SkillDraft> {
  return invoke<SkillDraft>('generate_skill_draft', { host, representative });
}

/** 초안을 ~/.claude/skills/<slug>/SKILL.md 로 저장. 저장된 절대 경로 반환. */
export async function saveSkillDraft(slug: string, markdown: string): Promise<string> {
  return invoke<string>('save_skill_draft', { slug, markdown });
}
