<script lang="ts">
  import { dayActivity, type DayActivityData } from '../../api';
  import { compactTokens, hhmm, toolBars } from './day-activity';

  let { date }: { date: string } = $props();

  let data = $state<DayActivityData | null>(null);
  let expanded = $state(false);
  /** 전문을 펼친 세션들. 프롬프트는 한 줄로 잘리므로 클릭해 전체를 볼 수 있게 한다. */
  let openPrompts = $state(new Set<string>());
  let loadSequence = 0;

  const SESSION_HEAD = 3;

  function togglePrompt(id: string) {
    const next = new Set(openPrompts);
    if (!next.delete(id)) next.add(id);
    openPrompts = next;
  }

  $effect(() => {
    const sequence = ++loadSequence;
    const target = date;
    expanded = false;
    openPrompts = new Set();
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
            {#if s.first_prompt}
              <button
                class="prompt"
                class:open={openPrompts.has(s.session_id)}
                title={s.first_prompt}
                onclick={() => togglePrompt(s.session_id)}
              >{s.first_prompt}</button>
            {/if}
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
  /* 달력은 제자리에 두고 이 패널만 스크롤한다 — .side가 flex column이고 여기가 남은 높이를 받는다.
     gap은 .side가 준다(margin-top을 쓰면 스크롤 높이 계산에 섞인다). */
  .activity {
    flex: 1; min-height: 0; overflow-y: auto;
    background: var(--frame-bg); border-radius: var(--radius-m);
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
  /* 기본은 한 줄 요약, 클릭하면 전문. button이라 키보드로도 열린다. */
  .prompt {
    display: block; width: 100%; margin: 1px 0 0; padding: 0;
    border: none; background: none; font: inherit; font-size: 11px; text-align: left;
    color: var(--ink-soft); cursor: pointer;
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .prompt:hover { color: var(--ink); }
  .prompt.open {
    white-space: normal; overflow: visible; word-break: keep-all; overflow-wrap: anywhere;
    line-height: 1.5; color: var(--ink);
  }
  .more {
    border: none; background: none; font: inherit; font-size: 11px;
    color: var(--accent-strong); cursor: pointer; padding: 2px 0;
  }
</style>
