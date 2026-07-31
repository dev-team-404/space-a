<script lang="ts">
  import { dayActivity, type DayActivityData } from '../../api';
  import { compactTokens, hhmm, toolBars } from './day-activity';

  let { date }: { date: string } = $props();

  let data = $state<DayActivityData | null>(null);
  let expanded = $state(false);
  let loadSequence = 0;

  const SESSION_HEAD = 3;

  $effect(() => {
    const sequence = ++loadSequence;
    const target = date;
    expanded = false;
    data = null;
    dayActivity(target)
      .then((d) => { if (sequence === loadSequence) data = d; })
      .catch(() => { if (sequence === loadSequence) data = null; });
  });

  const bars = $derived(data ? toolBars(data.tools.by_kind, 5) : []);
  const cache = $derived(data ? data.summary.tok_cache_read + data.summary.tok_cache_create : 0);
  const chips = $derived(data ? [...data.tools.skills, ...data.tools.mcp_servers] : []);
  const shown = $derived(
    !data ? [] : expanded ? data.sessions : data.sessions.slice(0, SESSION_HEAD),
  );
  const rest = $derived(data ? Math.max(0, data.sessions.length - SESSION_HEAD) : 0);
</script>

{#if data}
  <div class="activity">
    <h4>{date}</h4>
    <p class="head">세션 <b>{data.summary.session_count}</b> · 도구 <b>{data.tools.total_calls}</b>회</p>

    <section>
      <h5>토큰</h5>
      <dl>
        <dt>입력</dt><dd>{compactTokens(data.summary.tok_input)}</dd>
        <dt>출력</dt><dd>{compactTokens(data.summary.tok_output)}</dd>
        <dt>캐시</dt><dd>{compactTokens(cache)}</dd>
      </dl>
    </section>

    {#if bars.length}
      <section>
        <h5>도구 호출</h5>
        {#each bars as b (b.kind)}
          <div class="bar-row">
            <span class="kind">{b.kind}</span>
            <span class="track"><span class="fill" style:width={`${b.pct}%`}></span></span>
            <span class="count">{b.count}</span>
          </div>
        {/each}
      </section>
    {/if}

    {#if chips.length}
      <section>
        <h5>스킬 · MCP</h5>
        <div class="chips">{#each chips as c (c)}<span class="chip">{c}</span>{/each}</div>
      </section>
    {/if}

    {#if shown.length}
      <section>
        <h5>세션</h5>
        {#each shown as s (s.session_id)}
          <div class="session">
            <span class="when">{hhmm(s.first_ts)}</span>
            <span class="proj">{s.project}</span>
            {#if s.first_prompt}<p class="prompt">{s.first_prompt}</p>{/if}
          </div>
        {/each}
        {#if rest > 0 && !expanded}
          <button class="more" onclick={() => (expanded = true)}>… {rest}개 더 ▾</button>
        {/if}
      </section>
    {/if}
  </div>
{/if}

<style>
  .activity {
    margin-top: 12px; background: var(--frame-bg); border-radius: var(--radius-m);
    box-shadow: var(--shadow-soft); padding: 12px; font-size: 11px; color: var(--ink);
  }
  .activity h4 { margin: 0 0 6px; font-size: 12px; }
  .head { margin: 0 0 10px; color: var(--ink-soft); }
  .head b { color: var(--ink); }
  section { border-top: 1px solid var(--line); padding-top: 8px; margin-top: 8px; }
  h5 { margin: 0 0 6px; font-size: 11px; color: var(--ink-soft); font-weight: normal; }
  dl { display: grid; grid-template-columns: auto 1fr; gap: 2px 8px; margin: 0; }
  dt { color: var(--ink-soft); }
  dd { margin: 0; text-align: right; }
  .bar-row { display: grid; grid-template-columns: 44px 1fr auto; gap: 6px; align-items: center; margin-bottom: 3px; }
  .kind { color: var(--ink-soft); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .track { height: 6px; border-radius: 999px; background: var(--pastel-lav); overflow: hidden; }
  .fill { display: block; height: 100%; background: var(--accent); border-radius: 999px; }
  .count { color: var(--ink-soft); }
  .chips { display: flex; flex-wrap: wrap; gap: 4px; }
  .chip {
    background: var(--accent-tint); border-radius: var(--radius-s); padding: 1px 6px;
    max-width: 100%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .session { margin-bottom: 6px; }
  .when { color: var(--ink-soft); margin-right: 5px; }
  .proj { font-weight: 600; }
  .prompt {
    margin: 1px 0 0; color: var(--ink-soft); overflow: hidden;
    text-overflow: ellipsis; white-space: nowrap;
  }
  .more {
    border: none; background: none; font: inherit; font-size: 11px;
    color: var(--accent-strong); cursor: pointer; padding: 2px 0;
  }
</style>
