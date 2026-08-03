<script lang="ts">
  import type { CoachWidgetRow } from '../coach-stream';
  let { rows, onGoto }: { rows: CoachWidgetRow[]; onGoto: (key: string) => void } = $props();
</script>

<div class="widget">
  <h3>지금 볼 코칭</h3>
  {#if rows.length === 0}
    <p class="empty">지금은 지적할 게 없어요. 완벽해요!</p>
  {:else}
    <ol>
      {#each rows as r (r.key)}
        <li>
          <button class={r.kind} onclick={() => onGoto(r.key)}>
            <span class="badge">{r.badge}</span>
            <span class="action">{r.oneLine}</span>
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
    /* 탭과 같은 시각 언어 — 좌측 라인 색이 분류를 뜻한다 (§2.4) */
    border-left: 3px solid var(--pastel-mint);
  }
  li button.learning { border-left-color: var(--pastel-lav); }
  li button.news { border-left-color: var(--accent); }
  li button:hover { background: var(--pastel-lav); }
  /* 순위 숫자를 분류 배지로 바꿨다 — 스트림이 시간순이라 1·2·3에 뜻이 없다 */
  .badge { color: var(--accent-strong); font-weight: 700; white-space: nowrap; flex: none; }
  .action { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
</style>
