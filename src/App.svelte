<script lang="ts">
  import HomeTab from './lib/ui/HomeTab.svelte';
  import CoachTab from './lib/ui/CoachTab.svelte';
  import { getSummary, onScanDone, onGotoTab, type Summary } from './lib/api';

  let tab = $state<'home' | 'diary' | 'coach' | 'chat'>('home');
  let summary = $state<Summary | null>(null);

  async function refresh() {
    summary = await getSummary();
  }
  refresh();
  onScanDone(() => refresh());
  onGotoTab((t) => {
    if (t === 'home' || t === 'diary' || t === 'coach' || t === 'chat') tab = t;
  });

  const mood = $derived(
    summary && summary.est_tokens_saved_total > 0 ? '절약할 게 보여요…' : '평화로워요'
  );
</script>

<div class="shell">
  <aside class="side">
    <div class="counter">
      TODAY <b>{summary?.session_count ?? '–'}</b> · TOTAL <b>{summary?.total_sessions ?? '–'}</b>
    </div>
    <div class="miniroom">미니룸<br /><span class="robot">🤖</span><br /><small>(2단계 입주 예정)</small></div>
    <div class="mood">오늘의 기분: {mood}</div>
  </aside>
  <main class="main">
    <nav class="tabs">
      <button class:active={tab === 'home'} onclick={() => (tab = 'home')}>홈</button>
      <button class:active={tab === 'diary'} onclick={() => (tab = 'diary')}>다이어리</button>
      <button class:active={tab === 'coach'} onclick={() => (tab = 'coach')}>코칭</button>
      <button class:active={tab === 'chat'} onclick={() => (tab = 'chat')}>채팅</button>
    </nav>
    {#if tab === 'home'}
      <HomeTab {summary} />
    {:else if tab === 'coach'}
      <CoachTab />
    {:else}
      <section class="placeholder">준비 중이에요, 주인. (다음 단계에서 열려요)</section>
    {/if}
  </main>
</div>

<style>
  :global(body) {
    margin: 0;
    background: #f3f0e6;
    color: #33325a;
    font-family: 'Galmuri11', 'DungGeunMo', 'Courier New', monospace;
    font-size: 14px;
  }
  .shell { display: flex; height: 100vh; }
  .side {
    width: 200px; padding: 12px; background: #e8e4f0;
    border-right: 3px solid #33325a; display: flex; flex-direction: column; gap: 12px;
  }
  .counter { font-size: 12px; }
  .miniroom {
    border: 3px solid #33325a; background: #fffdf5; text-align: center;
    padding: 16px 8px; box-shadow: 4px 4px 0 #c9c3dd;
  }
  .robot { font-size: 40px; }
  .mood { font-size: 12px; margin-top: auto; }
  .main { flex: 1; display: flex; flex-direction: column; }
  .tabs { display: flex; gap: 4px; padding: 8px 8px 0; border-bottom: 3px solid #33325a; }
  .tabs button {
    border: 3px solid #33325a; border-bottom: none; background: #d9d4e8;
    padding: 6px 14px; font: inherit; cursor: pointer;
  }
  .tabs button.active { background: #fffdf5; }
  .placeholder { padding: 24px; }
</style>
