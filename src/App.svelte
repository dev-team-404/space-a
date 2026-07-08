<script lang="ts">
  import './lib/theme.css';
  import HomeTab from './lib/ui/HomeTab.svelte';
  import CoachTab from './lib/ui/CoachTab.svelte';
  import DiaryTab from './lib/ui/DiaryTab.svelte';
  import ChatTab from './lib/ui/ChatTab.svelte';
  import RobotPortrait from './lib/ui/RobotPortrait.svelte';
  import {
    getSummary, getDailyLine, listFindings, onScanDone, onGotoTab,
    onNewFindings, onDiaryReady, onOccasionToday, onDailyLine, type Summary,
  } from './lib/api';
  import { loadNotices, pushNotice, saveNotices, type Notice } from './lib/notices';

  type Tab = 'home' | 'diary' | 'coach' | 'chat';
  const TABS: { id: Tab; label: string }[] = [
    { id: 'home', label: '홈' },
    { id: 'diary', label: '다이어리' },
    { id: 'coach', label: '코칭' },
    { id: 'chat', label: '채팅' },
  ];

  let tab = $state<Tab>('home');
  let summary = $state<Summary | null>(null);
  let dailyLine = $state<string | null>(null);
  let activeCount = $state(0);
  let coachFocus = $state<string | null>(null);
  let notices = $state<Notice[]>(loadNotices());

  async function refresh() {
    summary = await getSummary().catch(() => null);
    activeCount = (await listFindings(false).catch(() => [])).length;
    dailyLine = await getDailyLine().catch(() => null);
  }
  refresh();
  onScanDone(() => refresh());
  onGotoTab((t) => {
    if (t === 'home' || t === 'diary' || t === 'coach' || t === 'chat') tab = t;
  });

  // 알림 히스토리 기록 (스펙 §2 — 창이 숨김이어도 수신됨)
  function record(kind: Notice['kind'], text: string) {
    notices = pushNotice(notices, { ts: new Date().toISOString(), kind, text });
    saveNotices(notices);
  }
  $effect(() => {
    const subs = [
      onNewFindings((rows) => rows.length && record('finding', `코칭 지적 ${rows.length}건이 도착했어요`)),
      onDiaryReady((date) => record('diary', `${date} 일기가 나왔어요`)),
      onOccasionToday((labels) => labels.length && record('occasion', `오늘은 ${labels[0]}!`)),
      onDailyLine((text) => { dailyLine = text; }),
    ];
    return () => { subs.forEach((p) => p.then((u) => u())); };
  });

  function gotoCoach(dedupKey: string) {
    coachFocus = dedupKey;
    tab = 'coach';
  }

  const mood = $derived(
    summary && summary.est_tokens_saved_total > 0 ? '절약할 게 보여요…' : '평화로워요'
  );
</script>

<div class="wall">
  <div class="homepy">
    <header class="titlebar">
      <h1>{summary?.user_name ?? '주인'}님의 미니홈피</h1>
      <div class="counter">
        TODAY <b>{summary?.session_count ?? '–'}</b> · TOTAL <b>{summary?.total_sessions ?? '–'}</b>
      </div>
    </header>
    <div class="body">
      <aside class="profile">
        <RobotPortrait />
        {#if dailyLine}<p class="daily-line">“{dailyLine}”</p>{/if}
        <p class="mood">“{mood}”</p>
      </aside>
      <main class="content">
        {#if tab === 'home'}
          <HomeTab {summary} onGotoCoach={gotoCoach} />
        {:else if tab === 'coach'}
          <CoachTab focusKey={coachFocus} onChanged={refresh} />
        {:else if tab === 'diary'}
          <DiaryTab />
        {:else}
          <ChatTab />
        {/if}
      </main>
      <nav class="tabs">
        {#each TABS as t (t.id)}
          <button class:active={tab === t.id} onclick={() => (tab = t.id)}>
            <span class="label">{t.label}</span>
            {#if t.id === 'coach' && activeCount > 0}<span class="badge">{activeCount}</span>{/if}
          </button>
        {/each}
      </nav>
    </div>
  </div>
</div>

<style>
  :global(html, body) { margin: 0; height: 100%; }
  :global(body) {
    background: var(--bg-grad);
    color: var(--ink);
    font-family: 'Segoe UI', 'Malgun Gothic', sans-serif;
    font-size: 14px;
  }
  .wall { height: 100vh; padding: 18px 34px 18px 18px; box-sizing: border-box; }
  .homepy {
    height: 100%; display: flex; flex-direction: column;
    background: var(--frame-bg);
    border-radius: var(--radius-l);
    box-shadow: var(--shadow-soft);
  }
  .titlebar {
    display: flex; justify-content: space-between; align-items: center;
    padding: 12px 20px;
    border-bottom: 1px solid var(--pastel-lav);
  }
  .titlebar h1 { margin: 0; font-size: 16px; font-weight: 600; }
  .counter { font-size: 12px; color: var(--ink-soft); }
  .counter b { color: var(--accent); }
  .body { flex: 1; display: flex; min-height: 0; position: relative; }
  .profile {
    width: 168px; padding: 16px 14px;
    border-right: 1px solid var(--pastel-lav);
    display: flex; flex-direction: column; gap: 12px;
  }
  .mood { margin: 0; font-size: 12px; color: var(--ink-soft); text-align: center; }
  .daily-line { margin: 0; font-size: 13px; color: var(--ink); text-align: center; line-height: 1.45; }
  /* margin-right: 스크롤바를 프레임 가장자리(우측 세로 탭이 걸치는 곳)에서 안쪽으로 밀어냄 */
  .content { flex: 1; min-width: 0; overflow-y: auto; display: flex; flex-direction: column; margin-right: 10px; }
  .tabs {
    position: absolute; right: -30px; top: 24px;
    display: flex; flex-direction: column; gap: 6px;
  }
  .tabs button {
    writing-mode: vertical-rl;
    border: none; cursor: pointer; font: inherit; font-size: 12px;
    padding: 12px 7px;
    background: var(--pastel-lav); color: var(--ink);
    border-radius: 0 var(--radius-s) var(--radius-s) 0;
    box-shadow: var(--shadow-soft);
    display: flex; align-items: center; gap: 4px;
  }
  .tabs button.active { background: var(--frame-bg); font-weight: 600; color: var(--accent); }
  .badge {
    writing-mode: horizontal-tb;
    background: var(--pastel-coral); color: var(--ink);
    border-radius: 999px; font-size: 10px; padding: 1px 5px;
  }
</style>
