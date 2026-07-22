<script lang="ts">
  import { getSessionTranscript, type TranscriptEntry } from '../api';

  let { sessionId, title, onClose }: { sessionId: string; title: string; onClose: () => void } = $props();

  let entries = $state<TranscriptEntry[] | null>(null);
  let error = $state<string | null>(null);

  getSessionTranscript(sessionId)
    .then((e) => (entries = e))
    .catch((e) => (error = String(e)));

  const when = (ts: string | null) => {
    if (!ts) return '';
    const d = new Date(ts);
    return `${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')} ${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`;
  };
</script>

<svelte:window onkeydown={(e) => e.key === 'Escape' && onClose()} />

<div class="overlay">
  <div class="modal" role="dialog" aria-label="세션 상세">
    <header>
      <b>{title}</b>
      <button class="x" onclick={onClose} aria-label="닫기">×</button>
    </header>
    <div class="list">
      {#if error}
        <p class="empty">{error}</p>
      {:else if entries === null}
        <p class="empty">원본을 불러오는 중…</p>
      {:else if entries.length === 0}
        <p class="empty">표시할 대화가 없어요.</p>
      {:else}
        {#each entries as e, i (i)}
          <article class:assistant={e.role === 'assistant'}>
            <div class="meta">
              <span class="who">{e.role === 'user' ? '👤 주인' : `🤖 ${e.model ?? 'assistant'}`}</span>
              {#if e.ts}<time>{when(e.ts)}</time>{/if}
            </div>
            {#if e.text}<p class="text">{e.text}</p>{/if}
            {#if e.tools.length}
              <ul class="tools">
                {#each e.tools as t, j (j)}<li>{t}</li>{/each}
              </ul>
            {/if}
          </article>
        {/each}
      {/if}
    </div>
  </div>
</div>

<style>
  .overlay {
    position: fixed; inset: 0; z-index: 50;
    background: rgba(74, 70, 104, 0.35);
    display: flex; align-items: center; justify-content: center;
  }
  .modal {
    width: min(760px, 92vw); height: min(82vh, 640px);
    background: var(--frame-bg); border-radius: var(--radius-l); box-shadow: var(--shadow-soft);
    display: flex; flex-direction: column; overflow: hidden;
  }
  header {
    display: flex; justify-content: space-between; align-items: center;
    padding: 12px 16px; border-bottom: 1px solid var(--pastel-lav);
  }
  .x {
    border: none; background: none; cursor: pointer;
    font-size: 18px; line-height: 1; color: var(--ink-soft); padding: 0 4px;
  }
  .x:hover { color: var(--ink); }
  .list { flex: 1; overflow-y: auto; padding: 12px 16px; display: flex; flex-direction: column; gap: 8px; }
  .empty { color: var(--ink-soft); }
  article {
    border-left: 3px solid var(--pastel-cream);
    background: var(--cream); color: var(--cream-ink);
    border-radius: var(--radius-s); padding: 8px 10px;
  }
  article.assistant { border-left-color: var(--accent); background: var(--accent-tint); }
  .meta { display: flex; justify-content: space-between; font-size: 11px; color: var(--ink-soft); }
  .who { font-weight: 600; }
  .text { margin: 4px 0 0; font-size: 12px; white-space: pre-wrap; word-break: break-word; }
  .tools { list-style: none; margin: 6px 0 0; padding: 0; display: flex; flex-direction: column; gap: 2px; }
  .tools li {
    font-size: 11px; font-family: Consolas, monospace; color: var(--ink-soft);
    background: var(--pastel-cream); border-radius: var(--radius-s); padding: 2px 6px;
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
</style>
