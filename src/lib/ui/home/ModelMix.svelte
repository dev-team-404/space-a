<script lang="ts">
  import type { ModelMixEntry } from '../../api';
  let { mix }: { mix: ModelMixEntry[] } = $props();
  const total = $derived(Math.max(mix.reduce((a, m) => a + m.tokens, 0), 1));
  // tier 필드에 raw 모델 id가 담긴다(구 데이터만 family 폴백). 계열별 고정색 + 나머지는 순환 팔레트.
  const FAMILY_COLOR: [string, string][] = [
    ['opus', 'var(--pastel-coral)'], ['sonnet', 'var(--pastel-lav)'],
    ['haiku', 'var(--pastel-mint)'], ['fable', 'var(--pastel-cream)'],
  ];
  const FALLBACK = ['#e3d3ec', '#cfe3d3', '#ecdccf', '#d3d9ec'];
  const color = (model: string, i: number) =>
    FAMILY_COLOR.find(([k]) => model.includes(k))?.[1] ?? FALLBACK[i % FALLBACK.length];
  const label = (model: string) => model.replace(/^claude-/, '');
  const pct = (t: number) => Math.round((t / total) * 100);
</script>

<div class="widget">
  <h3>오늘 모델 분포</h3>
  {#if mix.length === 0}
    <p class="empty">아직 오늘 기록이 없어요</p>
  {:else}
    <div class="stack">
      {#each mix as m, i (m.tier)}
        <div class="seg" style:width={`${pct(m.tokens)}%`} style:background={color(m.tier, i)}></div>
      {/each}
    </div>
    <ul class="legend">
      {#each mix as m, i (m.tier)}
        <li>
          <span class="chip" style:background={color(m.tier, i)}></span>
          {label(m.tier)} {pct(m.tokens)}% <small>({m.tokens.toLocaleString()})</small>
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .widget { background: var(--frame-bg); border-radius: var(--radius-m); box-shadow: var(--shadow-soft); padding: 12px 14px; }
  h3 { margin: 0 0 10px; font-size: 12px; color: var(--ink-soft); font-weight: 600; }
  .empty { margin: 0; font-size: 12px; color: var(--ink-soft); }
  .stack { display: flex; height: 14px; border-radius: 999px; overflow: hidden; }
  .legend { list-style: none; margin: 8px 0 0; padding: 0; display: flex; flex-wrap: wrap; gap: 8px; font-size: 11px; color: var(--ink-soft); }
  .legend li { display: flex; align-items: center; gap: 4px; }
  .chip { width: 10px; height: 10px; border-radius: 3px; display: inline-block; }
</style>
