<script lang="ts">
  import { listFindings, listContent, onContentReady, onNewFindings, onScanDone, setFindingStatus, sessionsCtx, generateSkillDraft, saveSkillDraft, getSettings, coachTip, setContentStatus, type CoachFinding, type ContentItem, type SessionCtxItem, type SkillDraft } from '../api';
  import SessionModal from './SessionModal.svelte';
  import LogCard from './coach/LogCard.svelte';
  import LearnCard from './coach/LearnCard.svelte';
  import { ctxLine, partitionCoachItems, sessionIdsOf, toDisposedRows, totalSessionsOf, type DisposedRow } from './coach-helpers';

  let { focusKey = null, onChanged }: { focusKey?: string | null; onChanged?: () => void } = $props();

  let all = $state<CoachFinding[]>([]);
  let open = $state<string | null>(null);
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

  // 큐레이션 콘텐츠 — personal 태그는 「내 로그에서」로, 나머지는 「배움 · 소식」으로 갈린다.
  let tips = $state<ContentItem[]>([]);
  // 처분된 레슨까지 포함한 전체 — 접힌 줄의 재료다. 활성 목록(tips)은 백엔드가 점수·축
  // 쿨다운으로 이미 걸러 주므로 그대로 두고, 처분 줄만 별도로 받는다.
  let allTips = $state<ContentItem[]>([]);

  // 근거 출처로 두 섹션을 가른다 (스펙 §3). 분기 로직은 전부 coach-helpers의 순수 함수에 있다.
  const sections = $derived(partitionCoachItems(active, tips));
  const findingByKey = $derived(new Map(active.map((f) => [f.dedup_key, f])));
  // 7일 창은 시간이 흘러야 닫히는데 `Date.now()`는 반응성 의존이 아니다. 스캔은 로그 감시
  // 디바운스라 Claude Code 활동이 없으면 아예 돌지 않으므로, 탭을 띄워둔 채 경계를 넘으면
  // 만료된 줄이 그대로 남는다. 창이 7일이라 시계는 1시간 눈금이면 충분하다.
  let nowMs = $state(Date.now());
  $effect(() => {
    const t = setInterval(() => (nowMs = Date.now()), 60 * 60 * 1000);
    return () => clearInterval(t);
  });

  // 처분 줄 (스펙 §5) — 해결함은 7일, 무시는 영구. 판정은 전부 순수 함수 쪽에 있다.
  const disposed = $derived(toDisposedRows(all, allTips, nowMs));

  // 내부망이면 외부 링크를 숨긴다 (옛 컨테이너에서 이관 — 두 카드가 각각 부르지 않도록 여기서 한 번만)
  let showLinks = $state(true);
  getSettings().then((s) => (showLinks = s.docs_reachable !== 'false')).catch(() => {});

  // 엔진 맞춤 코칭 — 배움 섹션 최상단 1건에만. personal 항목은 이미 로그 섹션으로 갈라져 여기 오지 않는다.
  // $derived로 두어 top이 그대로면 effect가 다시 돌지 않는다 (dismiss마다 재호출 방지). 캐시화는 PR⑥.
  const learnTop = $derived(tips.find((t) => !(t.trigger_tags?.includes('personal') ?? false)) ?? null);
  let coaching = $state<string | null>(null);
  let coachLoading = $state(false);
  $effect(() => {
    const top = learnTop;
    coaching = null;
    if (!top) return;
    coachLoading = true;
    coachTip(top)
      .then((s) => { coaching = s?.trim() || null; })
      .catch(() => { coaching = null; })
      .finally(() => { coachLoading = false; });
  });

  async function dismissLearn(id: string) {
    try { await setContentStatus(id, 'dismissed'); } catch { /* 무시 */ }
    tips = tips.filter((t) => t.id !== id);
  }

  async function refresh() {
    all = await listFindings(true).catch(() => []);
  }
  async function refreshTips() {
    const [visible, everything] = await Promise.all([
      listContent(false).catch(() => []),
      listContent(true).catch(() => []),
    ]);
    tips = visible;
    allTips = everything;
  }
  refresh();
  refreshTips();
  $effect(() => {
    // scan:done까지 듣는 이유: 재발로 되살아난 카드는 **기존 dedup_key**라 coach:finding의
    // "새 finding" diff에 잡히지 않는다. 탭을 열어둔 채로도 복귀가 보이려면 스캔 끝에 다시 읽어야 한다.
    const subs = [
      onNewFindings(() => refresh()),
      onScanDone(() => refresh()),
      onContentReady(() => refreshTips()),
    ];
    return () => { subs.forEach((s) => s.then((u) => u())); };
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

  type Disposition = 'resolved' | 'dismissed' | 'new';

  async function mark(key: string, status: Disposition) {
    await setFindingStatus(key, status).catch(() => {});
    await refresh();
    onChanged?.(); // 셸의 절약 총액·코칭 배지 즉시 갱신
  }

  // 개인 레슨도 룰 카드와 같은 처분 모델을 쓴다 (스펙 §5.5) — 옛 `✕` 영구 소멸(D4)을 대체한다.
  async function markLesson(id: string, status: Disposition) {
    await setContentStatus(id, status).catch(() => {});
    await refreshTips();
    onChanged?.();
  }

  const undo = (d: DisposedRow) =>
    d.source === 'finding' ? mark(d.key, 'new') : markLesson(d.key, 'new');

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

  // R6 → SKILL.md 초안: 반복 지시를 재사용 스킬로 전환 ("반복 작업은 Skill로 전환된다")
  const repeatedPromptOf = (f: CoachFinding): string | null => {
    const ev = f.evidence as { repeated_prompt?: string } | null;
    return ev?.repeated_prompt ?? null;
  };
  const skillifiable = (f: CoachFinding): boolean =>
    f.rule_id === 'R6' && !!repeatedPromptOf(f);
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
    const suggested = f.judgment?.suggested_name ?? null;
    // A — 느슨한 묶음의 모든 변형(member_norms)을 넘겨, 초안이 카드가 센 세션 전체를 보게 한다.
    const ev = f.evidence as { member_norms?: string[] } | null;
    const memberNorms = ev?.member_norms ?? null;
    draft = { key: f.dedup_key, loading: true, result: null, error: null, savedPath: null, copied: false };
    try {
      const r = await generateSkillDraft(f.scope_host ?? '', rep, suggested, memberNorms);
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
  <!-- 스펙 §3: 「내 로그에서」가 0건이면 섹션 헤더 대신 빈 상태 문구를 쓰고
       「배움 · 소식」이 자연히 상단에 온다 — 배움 카드 유무와 무관하다. -->
  {#if sections.log.length === 0}
    <p class="empty">지적할 게 없어요, 주인. 완벽해요!</p>
  {/if}

  {#if sections.log.length > 0}
    <h2 class="section">내 로그에서 <span class="count">{sections.log.length}</span></h2>
    {#each sections.log as v (v.key)}
      {@const f = findingByKey.get(v.key)}
      <LogCard view={v} {showLinks}>
        {#snippet extra()}
          {#if f && f.scope_kind === 'session' && sessionLine(f)}
            <p class="session">📂 {sessionLine(f)}</p>
          {:else if f && f.scope_kind === 'project' && f.scope_project}
            <p class="session">📂 {f.scope_project}</p>
          {/if}
          {#if f && f.scope_kind === 'project' && sessionIdsOf(f.evidence).length > 0}
            <button class="raw-toggle" onclick={() => toggleExpand(f)}>
              {expanded === f.dedup_key ? '▾' : '▸'} 포함 세션 {totalSessionsOf(f.evidence, sessionIdsOf(f.evidence).length)}건
            </button>
            {#if expanded === f.dedup_key}
              <ul class="session-list">
                {#each ctxCache[f.dedup_key] ?? [] as s (s.session_id)}
                  <li><button onclick={() => (detail = { id: s.session_id, title: ctxLine(s) })}>{ctxLine(s)}</button></li>
                {/each}
                {#if totalSessionsOf(f.evidence, 0) > sessionIdsOf(f.evidence).length}
                  <li class="more">최신 {sessionIdsOf(f.evidence).length}건 표시 중 (전체 {totalSessionsOf(f.evidence, 0)}건)</li>
                {/if}
              </ul>
            {/if}
          {/if}
        {/snippet}
        {#snippet actions()}
          {#if f}
            {#if f.fix_command}
              <button class="cmd" onclick={() => copy(f)}>{copied === f.dedup_key ? '복사됨!' : `📋 ${f.fix_command}`}</button>
            {/if}
            {#if skillifiable(f)}
              <button class="skillify" onclick={() => makeDraft(f)}>🧩 스킬 초안 만들기</button>
            {/if}
            {#if f.scope_kind === 'session'}
              <button onclick={() => openDetail(f)}>세션 상세</button>
            {/if}
            <button onclick={() => mark(f.dedup_key, 'resolved')}>해결함</button>
            <button onclick={() => mark(f.dedup_key, 'dismissed')}>무시</button>
          {:else}
            <!-- 개인 레슨도 같은 두 처분 — 「해결함」은 방출이 멈추면 정리되고(§5.2),
                 「무시」는 영구다. 둘 다 하단 접힌 줄에서 실행취소할 수 있다. -->
            <button onclick={() => markLesson(v.key, 'resolved')}>해결함</button>
            <button onclick={() => markLesson(v.key, 'dismissed')}>무시</button>
          {/if}
        {/snippet}
        {#snippet footer()}
          {#if f}
            <!-- occurrences는 스캔 횟수라 "N회 관측"이 카드 본문에선 오독을 유발 → 여기 원본 데이터로 (스펙 §1.2 D5) -->
            <button class="raw-toggle" onclick={() => (open = open === f.dedup_key ? null : f.dedup_key)}>
              {open === f.dedup_key ? '▾' : '▸'} 원본 데이터
            </button>
            {#if open === f.dedup_key}
              <p class="why">스캔에서 {f.occurrences}회 관측 · 마지막 {f.last_seen ?? '–'}</p>
              <pre>{JSON.stringify(f.evidence, null, 2)}</pre>
            {/if}
          {/if}
        {/snippet}
      </LogCard>
    {/each}
  {/if}

  {#if sections.learn.length > 0}
    <h2 class="section">배움 · 소식</h2>
    {#each sections.learn as v, i (v.id)}
      <LearnCard
        view={v}
        {showLinks}
        coaching={i === 0 ? (coachLoading ? '맞춤 코칭 생각 중…' : coaching) : null}
        onDismiss={dismissLearn}
      />
    {/each}
  {/if}

  <!-- 처분 줄 (스펙 §5) — 토글 없이 바로 노출한다. "내가 뭘 처분했는지"가 보여야 「해결함」이
       사라지는 게 아니라 접히는 것임을 알 수 있다. 표시 **위치**는 단일 스트림 재설계가 정한다. -->
  {#if disposed.length > 0}
    {#each disposed as d (`${d.source}:${d.key}`)}
      <div class="disposed" data-key={d.key} title={d.detail ?? ''}>
        <span class="disposed-label">{d.label}</span>
        <span class="disposed-title">{d.title}</span>
        <button onclick={() => undo(d)}>실행취소</button>
      </div>
    {/each}
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
  .section {
    margin: 6px 0 0; font-size: 12px; font-weight: 700; color: var(--ink-soft);
    display: flex; align-items: baseline; gap: 6px;
  }
  .section .count { font-size: 11px; color: var(--accent-strong); }
  /* 카드 본문 스타일은 LogCard/LearnCard로 이관됐다. 여기 남은 건 처분 줄뿐. */
  .disposed {
    display: flex; align-items: baseline; gap: 8px;
    opacity: 0.6; font-size: 12px; color: var(--ink-soft); padding: 2px 2px 2px 4px;
  }
  .disposed-label { flex: none; }
  .disposed-title { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .disposed button {
    flex: none; border: none; background: none; cursor: pointer;
    font: inherit; font-size: 11px; color: var(--ink-soft); text-decoration: underline; padding: 0;
  }
  .why { margin: 6px 0 2px; font-size: 12px; color: var(--ink-soft); }
  .session { margin: 2px 0; font-size: 12px; color: var(--ink-soft); }
  .raw-toggle {
    margin-top: 8px; border: none; background: none; cursor: pointer;
    font: inherit; font-size: 11px; color: var(--ink-soft); padding: 0;
  }
  pre {
    background: var(--surface-inset); color: var(--text); border-radius: var(--radius-s);
    padding: 8px; overflow-x: auto; font-size: 11px; margin: 6px 0 0;
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
  .draft-msg.err { color: var(--danger); }
  .draft-body {
    background: var(--surface-inset); color: var(--text); border-radius: var(--radius-s); padding: 10px;
    overflow: auto; font-size: 11.5px; line-height: 1.5; margin: 0; white-space: pre-wrap; flex: 1;
  }
  .draft-actions { display: flex; gap: 8px; margin-top: 10px; }
  .draft-actions button {
    border: none; cursor: pointer; font: inherit; font-size: 12px;
    background: var(--pastel-lav); color: var(--ink); border-radius: var(--radius-s); padding: 6px 12px;
  }
  .draft-actions .primary { background: var(--pastel-mint); font-weight: 600; }
  .draft-saved { font-size: 11px; color: var(--ink-soft); margin: 8px 0 0; word-break: break-all; }
  .draft-saved code { background: var(--surface-inset); padding: 1px 4px; border-radius: 3px; }
</style>
