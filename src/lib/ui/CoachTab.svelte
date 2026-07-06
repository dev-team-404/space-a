<script lang="ts">
  import { listFindings, onNewFindings, setFindingStatus, sessionsCtx, type CoachFinding, type SessionCtxItem } from '../api';
  import SessionModal from './SessionModal.svelte';
  import { ctxLine, sessionIdsOf, totalSessionsOf } from './coach-helpers';

  let { focusKey = null }: { focusKey?: string | null } = $props();

  let all = $state<CoachFinding[]>([]);
  let open = $state<string | null>(null);
  let showHidden = $state(false);
  let copied = $state<string | null>(null);
  let detail = $state<{ id: string; title: string } | null>(null);
  let expanded = $state<string | null>(null);
  let ctxCache = $state<Record<string, SessionCtxItem[]>>({});

  async function toggleExpand(f: CoachFinding) {
    if (expanded === f.dedup_key) {
      expanded = null;
      return;
    }
    expanded = f.dedup_key;
    if (!ctxCache[f.dedup_key]) {
      ctxCache[f.dedup_key] = await sessionsCtx(sessionIdsOf(f.evidence)).catch(() => []);
    }
  }

  const active = $derived(all.filter((f) => f.status === 'new'));
  const hidden = $derived(all.filter((f) => f.status !== 'new'));

  async function refresh() {
    all = await listFindings(true).catch(() => []);
  }
  refresh();
  $effect(() => {
    const p = onNewFindings(() => refresh());
    return () => { p.then((u) => u()); };
  });

  // 홈 '절약 top3'에서 진입 시 해당 카드로 스크롤 (스펙 §2)
  $effect(() => {
    if (!focusKey || all.length === 0) return;
    const el = document.querySelector(`[data-key="${CSS.escape(focusKey)}"]`);
    el?.scrollIntoView({ behavior: 'smooth', block: 'center' });
    open = focusKey;
  });

  async function copy(f: CoachFinding) {
    if (!f.fix_command) return;
    try {
      await navigator.clipboard.writeText(f.fix_command);
    } catch {
      return; // 실패 시 '복사됨!' 오표시 금지
    }
    copied = f.dedup_key;
    setTimeout(() => (copied = null), 1500);
  }

  async function mark(f: CoachFinding, status: 'resolved' | 'dismissed' | 'new') {
    await setFindingStatus(f.dedup_key, status).catch(() => {});
    await refresh();
  }

  // 세션 스코프 finding의 "어떤 작업인지" — 프로젝트 · 시작 시각
  const sessionLine = (f: CoachFinding) => {
    if (!f.session) return null;
    const ts = f.session.first_ts;
    if (!ts) return f.session.project_id;
    const d = new Date(ts);
    return `${f.session.project_id} · ${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')} ${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')} 세션`;
  };

  function openDetail(f: CoachFinding) {
    detail = { id: f.scope_ref, title: sessionLine(f) ?? '세션 상세' };
  }

  const icon = (s: CoachFinding['severity']) => (s === 'warn' ? '⚠' : s === 'suggest' ? '💡' : 'ℹ');
  const TITLE: Record<string, string> = {
    R1: '안 쓰는 MCP 서버가 토큰을 먹고 있어요',
    R2: '안 쓰는 플러그인이 자리만 차지해요',
    R5: '같은 파일을 반복해서 읽고 있어요',
    R7: '이 프로젝트, 가벼운 작업엔 시작 모델을 낮춰보세요',
    R9: '웹 검색이 너무 잦아요',
    R10: '자동화 파이프라인이 Opus로 돌고 있어요',
    R11: '같은 호출이 반복 거부되는 것 같아요',
    R12: '설치해둔 스킬이 놀고 있어요',
  };
</script>

<section class="coach">
  {#if active.length === 0}
    <p class="empty">지적할 게 없어요, 주인. 완벽해요!</p>
  {:else}
    {#each active as f (f.dedup_key)}
      <article class="card" class:warn={f.severity === 'warn'} data-key={f.dedup_key}>
        <header>
          <span class="title">{icon(f.severity)} {TITLE[f.rule_id] ?? '아낄 수 있는 게 보여요'}</span>
          <span class="save">~{f.est_tokens_saved.toLocaleString()} tok</span>
        </header>
        <p class="why">{f.detail} · {f.occurrences}회 관측</p>
        {#if sessionLine(f)}
          <p class="session">📂 {sessionLine(f)}</p>
        {/if}
        {#if f.scope_kind === 'project' && sessionIdsOf(f.evidence).length > 0}
          <button class="raw-toggle" onclick={() => toggleExpand(f)}>
            {expanded === f.dedup_key ? '▾' : '▸'} 포함 세션 {totalSessionsOf(f.evidence, sessionIdsOf(f.evidence).length)}건
          </button>
          {#if expanded === f.dedup_key}
            <ul class="session-list">
              {#each ctxCache[f.dedup_key] ?? [] as s (s.session_id)}
                <li>
                  <button onclick={() => (detail = { id: s.session_id, title: ctxLine(s) })}>
                    {ctxLine(s)}
                  </button>
                </li>
              {/each}
              {#if totalSessionsOf(f.evidence, 0) > sessionIdsOf(f.evidence).length}
                <li class="more">최신 {sessionIdsOf(f.evidence).length}건 표시 중 (전체 {totalSessionsOf(f.evidence, 0)}건)</li>
              {/if}
            </ul>
          {/if}
        {/if}
        <p class="how">➜ {f.suggested_action}</p>
        <div class="actions">
          {#if f.fix_command}
            <button class="cmd" onclick={() => copy(f)}>
              {copied === f.dedup_key ? '복사됨!' : `📋 ${f.fix_command}`}
            </button>
          {/if}
          {#if f.scope_kind === 'session'}
            <button onclick={() => openDetail(f)}>세션 상세</button>
          {/if}
          <button onclick={() => mark(f, 'resolved')}>해결함</button>
          <button onclick={() => mark(f, 'dismissed')}>무시</button>
        </div>
        <button class="raw-toggle" onclick={() => (open = open === f.dedup_key ? null : f.dedup_key)}>
          {open === f.dedup_key ? '▾' : '▸'} 원본 데이터
        </button>
        {#if open === f.dedup_key}
          <pre>{JSON.stringify(f.evidence, null, 2)}</pre>
        {/if}
      </article>
    {/each}
  {/if}

  {#if hidden.length > 0}
    <button class="hidden-toggle" onclick={() => (showHidden = !showHidden)}>
      숨긴 항목 {hidden.length}개 {showHidden ? '접기' : '보기'}
    </button>
    {#if showHidden}
      {#each hidden as f (f.dedup_key)}
        <article class="card muted" data-key={f.dedup_key}>
          <header>
            <span class="title">{f.status === 'resolved' ? '✔ 해결함' : '✕ 무시'} · {TITLE[f.rule_id] ?? f.rule_id}</span>
            <button onclick={() => mark(f, 'new')}>다시 보기</button>
          </header>
          <p class="why">{f.detail}</p>
        </article>
      {/each}
    {/if}
  {/if}

  {#if detail}
    <SessionModal sessionId={detail.id} title={detail.title} onClose={() => (detail = null)} />
  {/if}
</section>

<style>
  .coach { padding: 14px 16px; overflow-y: auto; display: flex; flex-direction: column; gap: 10px; }
  .empty { color: var(--ink-soft); }
  .card {
    background: var(--frame-bg); border-radius: var(--radius-m); box-shadow: var(--shadow-soft);
    padding: 12px 14px; border-left: 4px solid var(--pastel-mint);
  }
  .card.warn { border-left-color: var(--pastel-coral); }
  .card.muted { opacity: 0.75; border-left-color: var(--pastel-lav); }
  header { display: flex; justify-content: space-between; gap: 8px; align-items: baseline; }
  .title { font-weight: 600; }
  .save { color: var(--accent); font-size: 12px; white-space: nowrap; }
  .why { margin: 6px 0 2px; font-size: 12px; color: var(--ink-soft); }
  .session { margin: 2px 0; font-size: 12px; color: var(--ink-soft); }
  .how { margin: 2px 0 8px; font-size: 13px; }
  .actions { display: flex; gap: 6px; flex-wrap: wrap; }
  .actions button {
    border: none; cursor: pointer; font: inherit; font-size: 12px;
    background: var(--pastel-lav); color: var(--ink);
    border-radius: var(--radius-s); padding: 5px 10px;
  }
  .actions .cmd { background: var(--pastel-cream); font-family: Consolas, monospace; }
  .raw-toggle {
    margin-top: 8px; border: none; background: none; cursor: pointer;
    font: inherit; font-size: 11px; color: var(--ink-soft); padding: 0;
  }
  pre {
    background: #f4f1fa; border-radius: var(--radius-s);
    padding: 8px; overflow-x: auto; font-size: 11px; margin: 6px 0 0;
  }
  .hidden-toggle {
    align-self: flex-start; border: none; background: none; cursor: pointer;
    font: inherit; font-size: 12px; color: var(--ink-soft); text-decoration: underline; padding: 0;
  }
  .session-list { margin: 4px 0 0; padding: 0 0 0 8px; list-style: none; max-height: 180px; overflow-y: auto; }
  .session-list li { margin: 2px 0; }
  .session-list button {
    border: none; background: none; cursor: pointer; font: inherit;
    font-size: 12px; color: var(--ink-soft); text-decoration: underline; padding: 0;
  }
  .session-list .more { font-size: 11px; color: var(--ink-soft); }
  header button {
    border: none; cursor: pointer; font: inherit; font-size: 11px;
    background: var(--pastel-mint); border-radius: var(--radius-s); padding: 3px 8px;
  }
</style>
