<script lang="ts">
  import { marked } from 'marked';
  import DOMPurify from 'dompurify';
  import { getDiary, listDiaryDates, onDiaryReady } from '../api';
  import { monthGrid, shiftMonth } from './calendar';

  const now = new Date();
  let year = $state(now.getFullYear());
  let month = $state(now.getMonth() + 1);
  let dates = $state<Set<string>>(new Set());
  let selected = $state<string | null>(null);
  let html = $state<string | null>(null);

  const grid = $derived(monthGrid(year, month));

  async function loadDates() {
    dates = new Set(await listDiaryDates().catch(() => []));
  }
  loadDates();
  $effect(() => {
    const p = onDiaryReady(() => loadDates());
    return () => { p.then((u) => u()); };
  });

  async function pick(date: string) {
    if (!dates.has(date)) return;
    selected = date;
    const text = await getDiary(date).catch(() => null);
    // 일기는 우리 엔진(LLM) 산출물 — 웹뷰 주입 전 반드시 살균 (스펙 §4)
    html = text ? DOMPurify.sanitize(await marked.parse(text)) : null;
  }

  function nav(delta: number) {
    [year, month] = shiftMonth(year, month, delta);
  }
</script>

<section class="diary">
  <div class="cal">
    <header>
      <button onclick={() => nav(-1)}>‹</button>
      <b>{year}. {String(month).padStart(2, '0')}</b>
      <button onclick={() => nav(1)}>›</button>
    </header>
    <div class="dow">
      {#each ['일', '월', '화', '수', '목', '금', '토'] as d (d)}<span>{d}</span>{/each}
    </div>
    <div class="cells">
      {#each grid as c (c.date)}
        <button
          class:out={!c.inMonth}
          class:has={dates.has(c.date)}
          class:sel={selected === c.date}
          disabled={!dates.has(c.date)}
          onclick={() => pick(c.date)}
        >
          {c.day}{#if dates.has(c.date)}<i class="dot"></i>{/if}
        </button>
      {/each}
    </div>
  </div>
  <div class="body">
    {#if html}
      <article>{@html html}</article>
    {:else if selected}
      <p class="empty">이 날 일기를 불러오지 못했어요.</p>
    {:else}
      <p class="empty">도트 찍힌 날짜를 눌러 보세요. 그날의 일기가 나와요.</p>
    {/if}
  </div>
</section>

<style>
  .diary { display: flex; gap: 14px; padding: 14px 16px; flex: 1; min-height: 0; }
  .cal {
    width: 238px; flex-shrink: 0; align-self: flex-start;
    background: var(--frame-bg); border-radius: var(--radius-m); box-shadow: var(--shadow-soft);
    padding: 12px;
  }
  .cal header { display: flex; justify-content: space-between; align-items: center; margin-bottom: 8px; }
  .cal header button { border: none; background: var(--pastel-lav); border-radius: var(--radius-s); cursor: pointer; font: inherit; padding: 2px 9px; }
  .dow, .cells { display: grid; grid-template-columns: repeat(7, 1fr); }
  .dow span { text-align: center; font-size: 10px; color: var(--ink-soft); padding: 2px 0; }
  .cells button {
    position: relative; border: none; background: none; font: inherit; font-size: 11px;
    padding: 6px 0; color: var(--ink); border-radius: var(--radius-s);
  }
  .cells button.out { color: var(--pastel-lav); }
  .cells button.has { cursor: pointer; background: var(--pastel-cream); }
  .cells button.sel { background: var(--accent); color: #fff; }
  .cells button:disabled { cursor: default; }
  .dot {
    position: absolute; left: 50%; bottom: 1px; transform: translateX(-50%);
    width: 4px; height: 4px; border-radius: 50%; background: var(--accent);
  }
  .cells button.sel .dot { background: #fff; }
  .body { flex: 1; overflow-y: auto; min-width: 0; }
  .empty { color: var(--ink-soft); }
  article :global(h1), article :global(h2), article :global(h3) { font-size: 15px; }
</style>
