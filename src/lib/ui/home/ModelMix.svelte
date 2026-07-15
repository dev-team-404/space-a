<script lang="ts">
  import { getModelMix, onScanDone, type ModelMixEntry, type ModelMixPeriod } from '../../api';
  import { modelLabel, shapeMix } from './model-mix-helpers';

  const PERIODS: { key: ModelMixPeriod; label: string }[] = [
    { key: 'today', label: '오늘' }, { key: 'week', label: '주간' },
    { key: 'month', label: '월간' }, { key: 'all', label: '전체' },
  ];
  let period = $state<ModelMixPeriod>('today');
  let mix = $state<ModelMixEntry[]>([]);

  let seq = 0; // 토글 연타 시 마지막 요청 응답만 반영
  async function load() {
    const my = ++seq;
    const rows = await getModelMix(period).catch(() => [] as ModelMixEntry[]);
    if (my === seq) mix = rows;
  }
  load();
  $effect(() => {
    const sub = onScanDone(() => load());
    return () => { sub.then((u) => u()); };
  });
  function pick(p: ModelMixPeriod) {
    if (p === period) return;
    period = p;
    load();
  }

  const shaped = $derived(shapeMix(mix));
  // tier 필드에 raw 모델 id가 담긴다(구 데이터만 family 폴백). 계열별 고정색 + 나머지는 순환 팔레트.
  const FAMILY_COLOR: [string, string][] = [
    ['opus', 'var(--pastel-coral)'], ['sonnet', 'var(--pastel-lav)'],
    ['haiku', 'var(--pastel-mint)'], ['fable', 'var(--pastel-cream)'],
  ];
  const FALLBACK = ['#e3d3ec', '#cfe3d3', '#ecdccf', '#d3d9ec'];
  const OTHER_COLOR = '#d8d5d0'; // 기타(상위 3개 밖 합산) 세그먼트
  const color = (model: string, i: number) =>
    FAMILY_COLOR.find(([k]) => model.includes(k))?.[1] ?? FALLBACK[i % FALLBACK.length];
</script>

<div class="widget">
  <div class="head">
    <h3>모델 분포</h3>
    <div class="segs" role="group" aria-label="기간 선택">
      {#each PERIODS as p (p.key)}
        <button class:active={period === p.key} onclick={() => pick(p.key)}>{p.label}</button>
      {/each}
    </div>
  </div>
  {#if shaped.top.length === 0}
    <p class="empty">{period === 'today' ? '아직 오늘 기록이 없어요' : '이 기간엔 기록이 없어요'}</p>
  {:else}
    <div class="stack">
      {#each shaped.top as m, i (m.tier)}
        <div class="seg" style:width={`${m.pct}%`} style:background={color(m.tier, i)}></div>
      {/each}
      {#if shaped.other}
        <div class="seg" style:width={`${shaped.other.pct}%`} style:background={OTHER_COLOR}></div>
      {/if}
    </div>
    <ul class="legend">
      {#each shaped.top as m, i (m.tier)}
        <li class="has-tip">
          <span class="chip" style:background={color(m.tier, i)}></span>
          {modelLabel(m.tier)}
          <div class="tip" role="tooltip">
            <div class="row"><span class="pct">{m.pct.toFixed(1)}%</span></div>
          </div>
        </li>
      {/each}
      {#if shaped.other}
        <li class="has-tip">
          <span class="chip" style:background={OTHER_COLOR}></span>
          기타
          <div class="tip" role="tooltip">
            {#each shaped.other.items as it (it.tier)}
              <div class="row">
                <span class="name">{modelLabel(it.tier)}</span>
                <span class="pct">{it.pct.toFixed(1)}%</span>
              </div>
            {/each}
          </div>
        </li>
      {/if}
    </ul>
  {/if}
</div>

<style>
  .widget { background: var(--frame-bg); border-radius: var(--radius-m); box-shadow: var(--shadow-soft); padding: 12px 14px; }
  .head { display: flex; align-items: center; justify-content: space-between; margin: 0 0 10px; }
  h3 { margin: 0; font-size: 12px; color: var(--ink-soft); font-weight: 600; }
  .segs { display: flex; gap: 2px; }
  .segs button {
    border: none; cursor: pointer; font: inherit; font-size: 10px; color: var(--ink-soft);
    background: transparent; border-radius: 999px; padding: 2px 7px;
  }
  .segs button.active { background: var(--pastel-lav); color: var(--ink); }
  /* 박스 높이 고정 — 본문(stack 14 + 여백 8 + legend 36 = 58px)과 빈 상태를 같은 높이로 */
  .empty { margin: 0; font-size: 12px; color: var(--ink-soft); height: 58px; }
  .stack { display: flex; height: 14px; border-radius: 999px; overflow: hidden; }
  .legend {
    list-style: none; margin: 8px 0 0; padding: 0; display: flex; flex-wrap: wrap;
    gap: 8px; font-size: 11px; color: var(--ink-soft);
    height: 36px; align-content: flex-start;
  }
  .legend li { display: flex; align-items: center; gap: 4px; }
  .legend li.has-tip { position: relative; cursor: default; }
  .tip {
    display: none; position: absolute; left: 0; bottom: calc(100% + 6px); z-index: 5;
    background: var(--frame-bg); border: 1px solid rgba(0, 0, 0, 0.08);
    border-radius: var(--radius-s); box-shadow: var(--shadow-soft);
    padding: 6px 9px; white-space: nowrap; line-height: 1.6;
  }
  .has-tip:hover .tip { display: block; }
  .tip .row { display: flex; justify-content: space-between; gap: 14px; }
  .tip .name { color: var(--ink-soft); }
  .tip .pct { color: var(--ink); font-weight: 600; font-variant-numeric: tabular-nums; }
  .chip { width: 10px; height: 10px; border-radius: 3px; display: inline-block; }
</style>
