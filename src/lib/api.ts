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

export const getSummary = () => invoke<Summary>('get_summary');
export const listFindings = (includeHidden = false) =>
  invoke<CoachFinding[]>('list_findings', { includeHidden });
export const setFindingStatus = (dedupKey: string, status: 'new' | 'resolved' | 'dismissed') =>
  invoke<void>('set_finding_status', { dedupKey, status });
export const getWeekSummary = () => invoke<DayStat[]>('get_week_summary');
export const getModelMix = () => invoke<ModelMixEntry[]>('get_model_mix');
export const getTodayOccasions = () => invoke<string[]>('get_today_occasions');
export const runScanNow = () => invoke<void>('run_scan_now');
export const getMascotSeed = () => invoke<RobotSpec>('get_mascot_seed');
export const openChatTab = (tab: string) => invoke<void>('open_chat_tab', { tab });
export const getSettings = () => invoke<Record<string, string>>('get_settings');
export const setSetting = (key: string, value: string) => invoke<void>('set_setting', { key, value });
export const listDiaryDates = () => invoke<string[]>('list_diary_dates');
export const getDiary = (date: string) => invoke<string | null>('get_diary', { date });

// 마스코트가 pull한 occasion을 chat 창 알림 로그용으로 재방송 (pull 단일화 — 플랜 Task 2 Step 5)
export const emitOccasionToday = (labels: string[]) => emit('occasion:today', labels);

export const onScanDone = (cb: (ts: string) => void): Promise<UnlistenFn> =>
  listen<string>('scan:done', (e) => cb(e.payload));
export const onNewFindings = (cb: (rows: Finding[]) => void): Promise<UnlistenFn> =>
  listen<Finding[]>('coach:finding', (e) => cb(e.payload));
export const onDiaryReady = (cb: (date: string) => void): Promise<UnlistenFn> =>
  listen<string>('diary:ready', (e) => cb(e.payload));
export const onOccasionToday = (cb: (labels: string[]) => void): Promise<UnlistenFn> =>
  listen<string[]>('occasion:today', (e) => cb(e.payload));
export const onGotoTab = (cb: (tab: string) => void): Promise<UnlistenFn> =>
  listen<string>('chat:goto-tab', (e) => cb(e.payload));
export const onSettingsChanged = (cb: () => void): Promise<UnlistenFn> =>
  listen('settings:changed', () => cb());
