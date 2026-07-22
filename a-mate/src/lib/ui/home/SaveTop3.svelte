<script lang="ts">
  import type { CoachFinding } from '../../api';
  let { findings, onGoto }: { findings: CoachFinding[]; onGoto: (dedupKey: string) => void } = $props();
  const top3 = $derived(findings.slice(0, 3)); // 커맨드가 est_tokens_saved 내림차순 정렬을 보장
</script>

<div class="widget">
  <h3>절약 실천 top3</h3>
  {#if top3.length === 0}
    <p class="empty">지금은 지적할 게 없어요. 완벽해요!</p>
  {:else}
    <ol>
      {#each top3 as f, i (f.dedup_key)}
        <li>
          <button onclick={() => onGoto(f.dedup_key)}>
            <span class="rank">{i + 1}</span>
            <span class="action">{f.suggested_action}</span>
            <span class="save">~{f.est_tokens_saved.toLocaleString()}</span>
          </button>
        </li>
      {/each}
    </ol>
  {/if}
</div>

<style>
  .widget { background: var(--frame-bg); border-radius: var(--radius-m); box-shadow: var(--shadow-soft); padding: 12px 14px; }
  h3 { margin: 0 0 10px; font-size: 12px; color: var(--ink-soft); font-weight: 600; }
  .empty { margin: 0; font-size: 12px; color: var(--ink-soft); }
  ol { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 6px; }
  li button {
    width: 100%; display: flex; align-items: center; gap: 8px; text-align: left;
    border: none; cursor: pointer; font: inherit; font-size: 12px;
    background: var(--pastel-cream); color: var(--ink);
    border-radius: var(--radius-s); padding: 7px 10px;
  }
  li button:hover { background: var(--pastel-lav); }
  .rank { color: var(--accent-strong); font-weight: 700; }
  .action { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .save { color: var(--ink-soft); font-size: 11px; white-space: nowrap; }
</style>
