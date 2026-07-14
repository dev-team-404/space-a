<script lang="ts">
  import { setContentStatus, type ContentItem } from '../../api';

  let { items, onDismissed }: {
    items: ContentItem[];
    onDismissed: (id: string) => void;
  } = $props();

  // 사다리 축 → 한국어 배지 라벨
  const DIM_LABEL: Record<string, string> = {
    model_literacy: '모델 고르기',
    context_hygiene: '컨텍스트 정리',
    skill_reuse: '스킬로 반복 줄이기',
    automation: '워크플로 자동화',
    orchestration: '작업 위임',
  };
  const badge = (it: ContentItem) =>
    it.dimension ? (DIM_LABEL[it.dimension] ?? it.dimension) : '새 소식';

  // 최상위 팁(프론티어) 1건을 크게, 나머지는 접힌 목록으로
  const top = $derived(items[0] ?? null);
  const rest = $derived(items.slice(1, 4));

  async function dismiss(id: string) {
    try { await setContentStatus(id, 'dismissed'); } catch { /* 무시 */ }
    onDismissed(id);
  }
</script>

{#if top}
  <section class="tip">
    <div class="head">
      <span class="badge">{badge(top)}</span>
      <span class="label">오늘의 배움</span>
      <button class="x" title="이 팁 그만 보기" onclick={() => dismiss(top.id)}>✕</button>
    </div>
    <h3>{top.title}</h3>
    <p class="body">{top.body}</p>
    {#if top.source_url}
      <a class="more" href={top.source_url} target="_blank" rel="noreferrer">공식 가이드에서 더 배우기 →</a>
    {/if}

    {#if rest.length > 0}
      <ul class="rest">
        {#each rest as it (it.id)}
          <li>
            <span class="dot">·</span>
            {#if it.source_url}
              <a href={it.source_url} target="_blank" rel="noreferrer">{it.title}</a>
            {:else}
              <span>{it.title}</span>
            {/if}
            <span class="minibadge">{badge(it)}</span>
          </li>
        {/each}
      </ul>
    {/if}
  </section>
{/if}

<style>
  .tip {
    background: var(--frame-bg); border-radius: var(--radius-m); box-shadow: var(--shadow-soft);
    padding: 12px 14px; display: flex; flex-direction: column; gap: 7px;
    border-left: 3px solid var(--accent);
  }
  .head { display: flex; align-items: center; gap: 8px; }
  .badge {
    font-size: 10px; font-weight: 600; color: var(--ink);
    background: var(--pastel-mint); border-radius: 999px; padding: 2px 9px;
  }
  .label { font-size: 11px; color: var(--ink-soft); }
  .x {
    margin-left: auto; border: none; background: none; cursor: pointer;
    color: var(--ink-soft); font-size: 12px; padding: 2px 4px; line-height: 1;
  }
  .x:hover { color: var(--accent); }
  h3 { margin: 0; font-size: 14px; color: var(--ink); font-weight: 600; }
  .body { margin: 0; font-size: 12px; color: var(--ink-soft); line-height: 1.6; }
  .more { font-size: 11px; color: var(--accent); text-decoration: none; }
  .more:hover { text-decoration: underline; }
  .rest {
    list-style: none; margin: 4px 0 0; padding: 8px 0 0; border-top: 1px dashed var(--line, #e7e2ef);
    display: flex; flex-direction: column; gap: 4px; font-size: 12px;
  }
  .rest li { display: flex; align-items: baseline; gap: 5px; }
  .rest .dot { color: var(--ink-soft); }
  .rest a { color: var(--ink); text-decoration: none; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .rest a:hover { color: var(--accent); text-decoration: underline; }
  .minibadge { margin-left: auto; font-size: 9px; color: var(--ink-soft); white-space: nowrap; }
</style>
