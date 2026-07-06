<script lang="ts">
  import type { Notice } from '../../notices';
  let { notices }: { notices: Notice[] } = $props();
  const ICON: Record<Notice['kind'], string> = { finding: '💡', diary: '📓', occasion: '🎉' };
  const hhmm = (ts: string) => {
    const d = new Date(ts);
    return `${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`;
  };
</script>

<div class="widget">
  <h3>최근 알림</h3>
  {#if notices.length === 0}
    <p class="empty">아직 알림이 없어요</p>
  {:else}
    <ul>
      {#each notices.slice(0, 6) as n (n.ts + n.text)}
        <li><span>{ICON[n.kind]}</span><span class="text">{n.text}</span><time>{hhmm(n.ts)}</time></li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .widget { background: var(--frame-bg); border-radius: var(--radius-m); box-shadow: var(--shadow-soft); padding: 12px 14px; }
  h3 { margin: 0 0 10px; font-size: 12px; color: var(--ink-soft); font-weight: 600; }
  .empty { margin: 0; font-size: 12px; color: var(--ink-soft); }
  ul { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 5px; font-size: 12px; }
  li { display: flex; gap: 6px; align-items: baseline; }
  .text { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  time { color: var(--ink-soft); font-size: 10px; }
</style>
