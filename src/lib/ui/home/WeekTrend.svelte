<script lang="ts">
  import type { DayStat } from '../../api';
  let { days }: { days: DayStat[] } = $props();
  const max = $derived(Math.max(...days.map((d) => d.tok_input + d.tok_output), 1));
  const dow = (date: string) => '일월화수목금토'[new Date(date + 'T00:00:00').getDay()];
</script>

<div class="widget">
  <h3>주간 추이</h3>
  <div class="bars">
    {#each days as d (d.date)}
      <div class="col" title={`${d.date} · ${(d.tok_input + d.tok_output).toLocaleString()} tok · ${d.session_count}세션`}>
        <div class="bar" style:height={`${Math.round(((d.tok_input + d.tok_output) / max) * 100)}%`}></div>
        <span class="dow">{dow(d.date)}</span>
      </div>
    {/each}
  </div>
</div>

<style>
  .widget { background: var(--frame-bg); border-radius: var(--radius-m); box-shadow: var(--shadow-soft); padding: 12px 14px; }
  h3 { margin: 0 0 10px; font-size: 12px; color: var(--ink-soft); font-weight: 600; }
  .bars { display: flex; gap: 6px; align-items: flex-end; height: 72px; }
  .col { flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: flex-end; height: 100%; gap: 3px; }
  .bar { width: 100%; min-height: 2px; background: var(--pastel-lav); border-radius: 4px 4px 0 0; }
  .col:last-child .bar { background: var(--accent); }
  .dow { font-size: 10px; color: var(--ink-soft); }
</style>
