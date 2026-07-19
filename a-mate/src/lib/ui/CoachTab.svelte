<script lang="ts">
  import { listFindings, onNewFindings, setFindingStatus, sessionsCtx, generateSkillDraft, saveSkillDraft, type CoachFinding, type SessionCtxItem, type SkillDraft } from '../api';
  import SessionModal from './SessionModal.svelte';
  import { coachTitle, ctxLine, sessionIdsOf, totalSessionsOf } from './coach-helpers';

  let { focusKey = null, onChanged }: { focusKey?: string | null; onChanged?: () => void } = $props();

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
    onChanged?.(); // 셸의 절약 총액·코칭 배지 즉시 갱신
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

  // R6 → SKILL.md 초안: 반복 지시를 재사용 스킬로 전환 ("반복 작업은 Skill로 전환된다")
  const repeatedPromptOf = (f: CoachFinding): string | null => {
    const ev = f.evidence as { repeated_prompt?: string } | null;
    return ev?.repeated_prompt ?? null;
  };
  let draft = $state<{
    key: string;
    loading: boolean;
    result: SkillDraft | null;
    error: string | null;
    savedPath: string | null;
    copied: boolean;
  } | null>(null);

  async function makeDraft(f: CoachFinding) {
    const rep = repeatedPromptOf(f);
    if (!rep) return;
    draft = { key: f.dedup_key, loading: true, result: null, error: null, savedPath: null, copied: false };
    try {
      const r = await generateSkillDraft(f.scope_host ?? '', rep);
      draft = { key: f.dedup_key, loading: false, result: r, error: null, savedPath: null, copied: false };
    } catch (e) {
      draft = { key: f.dedup_key, loading: false, result: null, error: String(e), savedPath: null, copied: false };
    }
  }

  async function copyDraft() {
    if (!draft?.result) return;
    try {
      await navigator.clipboard.writeText(draft.result.markdown);
      draft = { ...draft, copied: true };
      setTimeout(() => { if (draft) draft = { ...draft, copied: false }; }, 1500);
    } catch { /* 복사 실패 무시 */ }
  }

  async function saveDraft() {
    if (!draft?.result) return;
    try {
      const path = await saveSkillDraft(draft.result.slug, draft.result.markdown);
      draft = { ...draft, savedPath: path, error: null };
    } catch (e) {
      draft = { ...draft, error: String(e) };
    }
  }
</script>

<svelte:window onkeydown={(e) => { if (e.key === 'Escape' && draft) draft = null; }} />

<section class="coach">
  {#if active.length === 0}
    <p class="empty">지적할 게 없어요, 주인. 완벽해요!</p>
  {:else}
    {#each active as f (f.dedup_key)}
      <article class="card" class:warn={f.severity === 'warn'} data-key={f.dedup_key}>
        <header>
          <span class="title">{icon(f.severity)} {coachTitle(f.rule_id, f.evidence)}</span>
          <span class="save">~{f.est_tokens_saved.toLocaleString()} tok</span>
        </header>
        <!-- 집계(project) 카드의 occurrences는 스캔 횟수라 "N회 관측"이 오독을 유발 → 원본 데이터로 이동 -->
        <p class="why">{f.detail}{#if f.scope_kind === 'session'} · {f.occurrences}회 관측{/if}</p>
        {#if sessionLine(f)}
          <p class="session">📂 {sessionLine(f)}</p>
        {:else if f.scope_kind === 'project' && f.scope_project}
          <p class="session">📂 {f.scope_project}</p>
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
          {#if f.rule_id === 'R6' && repeatedPromptOf(f)}
            <button class="skillify" onclick={() => makeDraft(f)}>🧩 스킬 초안 만들기</button>
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
          <p class="why">스캔에서 {f.occurrences}회 관측 · 마지막 {f.last_seen ?? '–'}</p>
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
            <span class="title">{f.status === 'resolved' ? '✔ 해결함' : '✕ 무시'} · {coachTitle(f.rule_id, f.evidence)}</span>
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

  {#if draft}
    <div class="draft-scrim">
      <div class="draft-modal" role="dialog" aria-modal="true" aria-label="스킬 초안">
        <header class="draft-head">
          <span>🧩 SKILL.md 초안</span>
          <button class="x" onclick={() => (draft = null)} aria-label="닫기">✕</button>
        </header>
        {#if draft.loading}
          <p class="draft-msg">반복 패턴을 스킬로 정리하는 중…</p>
        {:else if draft.error}
          <p class="draft-msg err">초안 생성 실패: {draft.error}</p>
        {:else if draft.result}
          <p class="draft-msg">
            같은 지시로 연 {draft.result.session_count}개 세션을 하나의 스킬로 묶었어요.
            {draft.result.llm_generated ? '' : '(엔진 미설정 — 기본 골격)'}
          </p>
          <pre class="draft-body">{draft.result.markdown}</pre>
          <div class="draft-actions">
            <button onclick={copyDraft}>{draft.copied ? '복사됨!' : '📋 복사'}</button>
            <button class="primary" onclick={saveDraft}>💾 스킬로 저장</button>
          </div>
          {#if draft.savedPath}
            <p class="draft-saved">저장됨 → <code>{draft.savedPath}</code></p>
          {/if}
        {/if}
      </div>
    </div>
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
  .how { margin: 2px 0 8px; font-size: 13px; white-space: pre-line; }
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
  .actions .skillify { background: var(--pastel-mint); font-weight: 600; }
  .draft-scrim {
    position: fixed; inset: 0; background: rgba(40, 30, 60, 0.35);
    display: flex; align-items: center; justify-content: center; z-index: 50; padding: 20px;
  }
  .draft-modal {
    background: var(--frame-bg); border-radius: var(--radius-m); box-shadow: var(--shadow-soft);
    width: min(560px, 100%); max-height: 82vh; display: flex; flex-direction: column; padding: 14px 16px;
  }
  .draft-head { display: flex; justify-content: space-between; align-items: center; font-weight: 600; }
  .draft-head .x { border: none; background: none; cursor: pointer; font: inherit; color: var(--ink-soft); }
  .draft-msg { font-size: 12px; color: var(--ink-soft); margin: 8px 0; }
  .draft-msg.err { color: var(--pastel-coral); }
  .draft-body {
    background: #f4f1fa; border-radius: var(--radius-s); padding: 10px;
    overflow: auto; font-size: 11.5px; line-height: 1.5; margin: 0; white-space: pre-wrap; flex: 1;
  }
  .draft-actions { display: flex; gap: 8px; margin-top: 10px; }
  .draft-actions button {
    border: none; cursor: pointer; font: inherit; font-size: 12px;
    background: var(--pastel-lav); color: var(--ink); border-radius: var(--radius-s); padding: 6px 12px;
  }
  .draft-actions .primary { background: var(--pastel-mint); font-weight: 600; }
  .draft-saved { font-size: 11px; color: var(--ink-soft); margin: 8px 0 0; word-break: break-all; }
  .draft-saved code { background: #f4f1fa; padding: 1px 4px; border-radius: 3px; }
</style>
