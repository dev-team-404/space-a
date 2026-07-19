<script lang="ts">
  import { getProfile, type ProfileView, type ProfileRung } from '../../api';

  let profile = $state<ProfileView | null>(null);
  let open = $state(true);

  async function load() {
    profile = await getProfile().catch(() => null);
  }
  load();

  const masteryLabel = (m: ProfileRung['mastery']) =>
    m === 'mastered' ? '숙달' : m === 'in_progress' ? '배우는 중' : '아직';
  const masteryIcon = (m: ProfileRung['mastery']) =>
    m === 'mastered' ? '✓' : m === 'in_progress' ? '◐' : '○';

  const frontier = $derived(profile?.rungs.find((r) => r.is_frontier) ?? null);
  const masteredCount = $derived(profile?.rungs.filter((r) => r.mastery === 'mastered').length ?? 0);
</script>

<section class="ladder">
  <header>
    <div class="title">
      <span>🎓 AX 역량 사다리</span>
      {#if profile}
        <span class="prog">{masteredCount}/{profile.rungs.length} 숙달</span>
      {/if}
    </div>
    <button class="fold" onclick={() => (open = !open)} aria-label={open ? '접기' : '펼치기'}>
      {open ? '▾' : '▸'}
    </button>
  </header>

  {#if profile && (profile.total_events ?? 0) === 0}
    <p class="empty">아직 데이터를 모으는 중이에요. 몇 번 세션을 돌리면 여기에 성장 지도가 그려져요.</p>
  {:else if profile}
    {#if frontier}
      <div class="frontier">
        <span class="badge">지금 배울 것</span>
        <div class="ftext">
          <b>{frontier.label}</b>
          <p>{frontier.learn_hint}</p>
        </div>
      </div>
    {:else}
      <p class="allmastered">🎉 5개 축을 모두 숙달했어요. 훌륭한 AX 마스터예요, 주인!</p>
    {/if}

    {#if open}
      <ol class="rungs">
        {#each profile.rungs as r (r.key)}
          <li class="rung" class:frontier={r.is_frontier} data-m={r.mastery}>
            <span class="lv">Lv{r.ladder_index}</span>
            <span class="dot">{masteryIcon(r.mastery)}</span>
            <div class="body">
              <div class="row">
                <span class="name">{r.label}</span>
                <span class="mtag {r.mastery}">{masteryLabel(r.mastery)}</span>
              </div>
              {#if r.evidence}
                <p class="evi">{r.evidence}</p>
              {/if}
              {#if r.is_frontier}
                <p class="hint">➜ {r.learn_hint}</p>
              {/if}
            </div>
          </li>
        {/each}
      </ol>
    {/if}
  {/if}
</section>

<style>
  .ladder {
    background: var(--frame-bg); border-radius: var(--radius-m); box-shadow: var(--shadow-soft);
    padding: 12px 14px; display: flex; flex-direction: column; gap: 10px;
  }
  header { display: flex; justify-content: space-between; align-items: center; }
  .title { display: flex; align-items: baseline; gap: 8px; font-weight: 700; font-size: 14px; }
  .prog { font-size: 11px; color: var(--accent); font-weight: 600; }
  .fold { border: none; background: none; cursor: pointer; font: inherit; color: var(--ink-soft); }
  .empty, .allmastered { margin: 0; font-size: 12px; color: var(--ink-soft); }
  .allmastered { color: var(--accent); }

  .frontier {
    display: flex; gap: 10px; align-items: flex-start;
    background: var(--pastel-cream); border-radius: var(--radius-s); padding: 10px 12px;
  }
  .badge {
    background: var(--pastel-mint); color: var(--ink); font-size: 10px; font-weight: 700;
    padding: 3px 8px; border-radius: 999px; white-space: nowrap; margin-top: 1px;
  }
  .ftext b { font-size: 13px; }
  .ftext p { margin: 2px 0 0; font-size: 12px; color: var(--ink-soft); }

  .rungs { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 2px; }
  .rung {
    display: grid; grid-template-columns: 34px 20px 1fr; align-items: start; gap: 8px;
    padding: 7px 6px; border-radius: var(--radius-s); position: relative;
  }
  .rung.frontier { background: color-mix(in srgb, var(--pastel-mint) 26%, transparent); }
  .lv { font-size: 10px; color: var(--ink-soft); font-weight: 700; padding-top: 2px; }
  .dot { font-size: 14px; line-height: 1.3; text-align: center; }
  .rung[data-m='mastered'] .dot { color: var(--accent); }
  .rung[data-m='in_progress'] .dot { color: var(--pastel-lav); }
  .rung[data-m='not_started'] .dot { color: var(--ink-soft); opacity: 0.5; }
  .row { display: flex; align-items: baseline; gap: 8px; }
  .name { font-size: 13px; font-weight: 600; }
  .mtag {
    font-size: 10px; padding: 1px 7px; border-radius: 999px; white-space: nowrap;
    background: var(--pastel-lav); color: var(--ink);
  }
  .mtag.mastered { background: var(--pastel-mint); }
  .mtag.not_started { background: transparent; color: var(--ink-soft); border: 1px solid var(--ink-soft); opacity: 0.7; }
  .evi { margin: 2px 0 0; font-size: 11px; color: var(--ink-soft); }
  .hint { margin: 3px 0 0; font-size: 12px; color: var(--ink); }
</style>
