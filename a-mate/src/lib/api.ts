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
  /** R6 판정 결과(worthy만 노출됨). rejected/pending은 목록에 안 옴. */
  judgment?: { worthy?: boolean; reason?: string; suggested_name?: string } | null;
}

export interface CoachFinding extends Finding {
  status: 'new' | 'resolved' | 'dismissed' | 'pending' | 'rejected';
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

// --- 방 방문 (docs/design/life-visit.md) ---

export interface HubSettings {
  url: string;
  user: string;
  connected: boolean;
  life_id: string;
}
export interface LifeOccupant {
  agent_id: string;
  name: string;
  cell: [number, number];
  is_owner: boolean;
  mascot_seed: string;
  mascot_image_sha256?: string | null;
  bubble?: string;
}
export interface LifeState {
  life_id: string;
  owner_agent_id: string;
  owner_name: string;
  owner_mascot_seed: string; // 주인이 방을 비워도 프로필 로봇을 그릴 수 있게 서버가 항상 준다
  owner_mascot_image_sha256?: string | null;
  grid: { w: number; h: number };
  design: { wallpaper: string; floor: string; objects: LifeObject[] };
  occupants: LifeOccupant[];
}
export interface LifeObject {
  asset_id: string;
  category: string;
  cell: [number, number];
  size: [number, number];
  footprint?: [number, number][];
  rotation: 0 | 90 | 180 | 270;
  wall?: 'north' | 'west' | null;
}
export interface LifeMe {
  agent_id: string;
  name: string;
  my_life_id: string;
  life_id: string;
  cell: [number, number];
}
export interface LifeListEntry {
  life_id: string;
  owner_name: string;
  occupants: number;
}
export interface LifeCapabilities { life_protocol: number; grid: { w: number; h: number }; floor_min_y: number; footprint_mask: boolean; wall_objects: boolean }

export const hubSettingsGet = () => invoke<HubSettings>('hub_settings_get');
export const hubConnect = (url: string, apiKey = '') =>
  invoke<HubSettings>('hub_connect', { url, apiKey });
export const hubDisconnect = () => invoke<HubSettings>('hub_disconnect');
type LifeViewResponse = { me: LifeMe; life: LifeState };
let lifeViewInFlight: Promise<LifeViewResponse> | null = null;
let lifeViewCache: { at: number; value: LifeViewResponse } | null = null;
const invalidateLifeView = () => { lifeViewCache = null; };
export const lifeView = () => {
  const now = performance.now();
  if (lifeViewCache && now - lifeViewCache.at < 500) return Promise.resolve(lifeViewCache.value);
  if (lifeViewInFlight) return lifeViewInFlight;
  lifeViewInFlight = invoke<LifeViewResponse>('life_view')
    .then((value) => {
      lifeViewCache = { at: performance.now(), value };
      return value;
    })
    .finally(() => { lifeViewInFlight = null; });
  return lifeViewInFlight;
};
export const lifeCapabilities = () => invoke<LifeCapabilities>('life_capabilities');
export const lifeList = () => invoke<{ life: LifeListEntry[] }>('life_list');
export const lifeGoto = (lifeId: string) => invoke<LifeMe>('life_goto', { lifeId }).then((value) => { invalidateLifeView(); return value; });
export const lifeMoveCell = (x: number, y: number) => invoke<LifeMe>('life_move_cell', { x, y }).then((value) => { invalidateLifeView(); return value; });
export const lifeSaveDesign = (lifeId: string, design: LifeState['design']) =>
  invoke<LifeState>('life_save_design', { lifeId, design }).then((value) => { invalidateLifeView(); return value; });
export interface LifePerson { agent_id: string; name: string; life_id: string; is_friend: boolean }
export interface SharedDiary { date: string; body: string; visibility: 'friends' | 'public' }
export interface GuestbookEntry { entry_id: string; life_id: string; author_agent_id: string; author_name: string; body: string; created_at: string }
export const lifePeople = () => invoke<{people: LifePerson[]}>('life_people');
export const lifeSetFriend = (agentId: string, enabled: boolean) => invoke('life_set_friend', { agentId, enabled });
export type ContentVisibility = 'private'|'friends'|'public';
export interface ContentAccess { features: { diary: { visibility: ContentVisibility; can_view: boolean } } }
export const lifeSetContentVisibility = (feature: string, visibility: ContentVisibility) =>
  invoke('life_set_content_visibility', { feature, visibility });
export const lifeContentAccess = (lifeId: string) => invoke<ContentAccess>('life_content_access', { lifeId });
export const lifeSetDiaryVisibility = (date: string, body: string, visibility: 'private'|'friends'|'public') =>
  invoke('life_set_diary_visibility', { date, body, visibility });
export const lifeDiaries = (lifeId: string) => invoke<{diaries: SharedDiary[]}>('life_diaries', { lifeId });
export const lifeGuestbook = (lifeId: string) => invoke<{entries: GuestbookEntry[]}>('life_guestbook', { lifeId });
export const lifeAddGuestbook = (lifeId: string, body: string) => invoke<GuestbookEntry>('life_add_guestbook', { lifeId, body });
export const lifeDeleteGuestbook = (entryId: string) => invoke('life_delete_guestbook', { entryId });
export const lifeSetBubble = (body: string) => invoke<{bubble:string}>('life_set_bubble', { body }).then((v)=>{invalidateLifeView();return v});
export const lifeSyncMascotImage = () => invoke<boolean>('life_sync_mascot_image');
export const lifeMascotImage = (agentId: string) => invoke<string | null>('life_mascot_image', { agentId });
export const robotSpecForSeed = (seed: string) => invoke<RobotSpec>('robot_spec_for_seed', { seed });
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
/** 트레이 "업데이트 확인" → chat 창에서 수동 업데이트 체크를 트리거 */
export const onUpdateCheckRequested = (cb: () => void): Promise<UnlistenFn> =>
  listen('update:check', () => cb());

/** AI 스프라이트(캐시) base64 — 없으면 null (절차 생성 폴백). */
export async function getSprite(): Promise<string | null> {
  try { return await invoke<string | null>('get_sprite'); } catch { return null; }
}

/** 방 점유자 AI 스프라이트(캐시) base64 — 없으면 null. */
export async function getOccupantSprite(seed: string): Promise<string | null> {
  try { return await invoke<string | null>('get_occupant_sprite', { seed }); } catch { return null; }
}

/** 텍스트 LLM 엔진 설정 — 일기·한마디·잡담·채팅이 쓰는 OpenAI 호환 엔드포인트. */
export interface EngineSettings { url: string; key: string; model: string; source: 'store' | 'env' | 'none' }

export const engineSettingsGet = () => invoke<EngineSettings>('engine_settings_get');
export const engineSettingsSet = (url: string, key: string, model: string) =>
  invoke<void>('engine_settings_set', { url, key, model });
/** 연결 확인 — 성공 시 사람이 읽는 메시지를 돌려준다. */
export const engineTest = (url: string, key: string, model: string) =>
  invoke<string>('engine_test', { url, key, model });

/** 캐릭터 이미지 모델 설정 (텍스트 엔진과 분리 — 사내 LLM은 이미지 생성을 못 하므로). */
export interface ImageSettings { url: string; key: string; model: string; source: 'store' | 'env' | 'none' }

export async function imageSettingsGet(): Promise<ImageSettings> {
  return await invoke<ImageSettings>('image_settings_get');
}

export async function imageSettingsSet(url: string, key: string, model: string): Promise<void> {
  await invoke('image_settings_set', { url, key, model });
}

/** 마스코트 후보를 새로 그려 미리보기 base64를 돌려준다(수십 초). 저장 전까지 실사용본 미변경. */
export async function mascotPreview(): Promise<string> {
  return await invoke<string>('mascot_preview');
}
/** 미리보기 후보를 실제 마스코트로 저장(전체 반영). 완료 시 sprite:ready. */
export async function mascotCommit(): Promise<void> {
  await invoke('mascot_commit');
}

/** 이미지 엔드포인트 검증 (무과금 — GET /models). 성공/실패 모두 사람이 읽는 메시지. 실패는 reject. */
export const imageTest = (url: string, key: string, model: string) =>
  invoke<string>('image_test', { url, key, model });

/** 개인정보 — 이름·조직·아이디(UUID 자동)·MBTI. a-hub 연결·마스코트 시드에 쓰인다. */
export interface Profile { name: string; org: string; uuid: string; mbti: string; owner_title: string }
export const profileGet = () => invoke<Profile>('profile_get');
export const profileSet = (name: string, org: string, mbti: string, ownerTitle: string) =>
  invoke<Profile>('profile_set', { name, org, mbti, ownerTitle });

/** 주인 메모리 — 마스코트가 기억하는 나에 대한 자유 텍스트 사실. */
export interface Memory {
  id: number;
  text: string;
  created_at: string;
  updated_at: string | null;
  source: string;
}
export const memoryList = () => invoke<Memory[]>('memory_list');
export const memoryAdd = (text: string) => invoke<Memory>('memory_add', { text });
export const memoryUpdate = (id: number, text: string) => invoke<Memory>('memory_update', { id, text });
export const memoryDelete = (id: number) => invoke<void>('memory_delete', { id });

/** 점유자 스프라이트 백그라운드 생성 요청 — 완료 시 'occupant-sprite:ready'(seed) 이벤트. */
export async function requestOccupantSprite(seed: string): Promise<void> {
  try { await invoke('request_occupant_sprite', { seed }); } catch { /* 무시 */ }
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

/** R6 반복 지시 → SKILL.md 초안 생성. 판정이 제안한 이름(suggestedName)을 기본 슬러그로 쓴다. */
export async function generateSkillDraft(
  host: string,
  representative: string,
  suggestedName: string | null = null,
  memberNorms: string[] | null = null,
): Promise<SkillDraft> {
  return invoke<SkillDraft>('generate_skill_draft', { host, representative, suggestedName, memberNorms });
}

/** 초안을 ~/.claude/skills/<slug>/SKILL.md 로 저장. 저장된 절대 경로 반환. */
export async function saveSkillDraft(slug: string, markdown: string): Promise<string> {
  return invoke<string>('save_skill_draft', { slug, markdown });
}
