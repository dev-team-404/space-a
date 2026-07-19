<script lang="ts">
  import { openUrl } from '@tauri-apps/plugin-opener';
  import { getSettings, setContentStatus, coachTip, type ContentItem } from '../../api';

  // 공식 가이드 링크를 시스템 브라우저로 연다 (Tauri 웹뷰는 외부 <a> 네비게이션을 막음).
  async function openExternal(url: string) {
    try { await openUrl(url); } catch (e) { console.error('openUrl 실패:', url, e); }
  }

  // 내부망 감지(docs_reachable=false)면 외부 가이드 링크를 숨긴다 (동료 이슈)
  let docsOk = $state(true);
  getSettings().then((s) => (docsOk = s.docs_reachable !== 'false')).catch(() => {});

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
    it.trigger_tags?.includes('personal')
      ? '내 로그 맞춤'
      : it.dimension ? (DIM_LABEL[it.dimension] ?? it.dimension) : '새 소식';

  // 최상위 팁(프론티어) 1건을 크게, 나머지는 접힌 목록으로
  const top = $derived(items[0] ?? null);
  const rest = $derived(items.slice(1, 4));
  // Boris(커뮤니티) 팁은 원문이 영어라, 한글 코칭(🤖)이 있으면 원문 본문을 숨긴다.
  const isBoris = $derived(top?.trigger_tags?.includes('boris') ?? false);

  // (2) LLM 코칭 — top 팁이 바뀌면 엔진에 맞춤 코칭을 요청(비동기). 엔진 미설정/실패면 조용히 생략.
  let coaching = $state<string | null>(null);
  let coachLoading = $state(false);
  $effect(() => {
    const t = top;
    coaching = null;
    if (!t) return;
    if (t.trigger_tags?.includes('personal')) return; // 레슨 본문이 이미 개인화 — 중복 코칭 금지
    coachLoading = true;
    coachTip(t)
      .then((s) => { coaching = s?.trim() || null; })
      .catch(() => { coaching = null; })
      .finally(() => { coachLoading = false; });
  });

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
    {#if top.personal}
      <p class="personal">📊 {top.personal}</p>
    {/if}
    {#if coachLoading}
      <p class="coach loading">🤖 맞춤 코칭 생각 중…</p>
    {:else if coaching}
      <p class="coach">🤖 {coaching}</p>
    {/if}
    {#if top.body && !(isBoris && (coaching || coachLoading))}
      <p class="body">{top.body}</p>
    {/if}
    {#if top.source_url && docsOk}
      <a class="more" href={top.source_url} onclick={(e) => { e.preventDefault(); openExternal(top.source_url!); }}>공식 가이드에서 더 배우기 →</a>
    {/if}

    {#if rest.length > 0}
      <ul class="rest">
        {#each rest as it (it.id)}
          <li>
            <span class="dot">·</span>
            {#if it.source_url && docsOk}
              <a href={it.source_url} onclick={(e) => { e.preventDefault(); openExternal(it.source_url!); }}>{it.title}</a>
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
    padding: 13px 15px; display: flex; flex-direction: column; gap: 8px;
    border: 1px solid var(--line); border-left: 3px solid var(--accent);
  }
  .head { display: flex; align-items: center; gap: 8px; }
  .badge {
    font-size: 10px; font-weight: 700; color: #0b3327; white-space: nowrap;
    background: var(--pastel-mint); border-radius: 999px; padding: 3px 10px;
  }
  .label { font-size: 11px; color: var(--ink-soft); }
  .x {
    margin-left: auto; border: none; background: none; cursor: pointer;
    color: var(--ink-soft); font-size: 12px; padding: 2px 4px; line-height: 1;
  }
  .x:hover { color: var(--accent); }
  h3 { margin: 0; font-size: 15px; color: var(--ink); font-weight: 700; line-height: 1.4; }
  .body { margin: 0; font-size: 12px; color: var(--ink-soft); line-height: 1.65; }
  .personal {
    margin: 0; font-size: 12px; color: var(--ink); line-height: 1.6; font-weight: 500;
    background: var(--mint-tint); border: 1px solid var(--mint-tint-b);
    border-radius: var(--radius-s); padding: 8px 11px;
  }
  .coach { margin: 0; font-size: 12.5px; color: var(--lav); line-height: 1.6; }
  .coach.loading { color: var(--ink-soft); font-style: italic; }
  .more { font-size: 12px; color: var(--accent); text-decoration: none; font-weight: 600; width: fit-content; }
  .more:hover { text-decoration: underline; }
  .rest {
    list-style: none; margin: 4px 0 0; padding: 9px 0 0; border-top: 1px dashed var(--line);
    display: flex; flex-direction: column; gap: 7px; font-size: 12px;
  }
  .rest li { display: flex; align-items: baseline; gap: 6px; }
  .rest .dot { color: var(--accent); flex: none; }
  .rest a { color: var(--ink); text-decoration: none; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; min-width: 0; }
  .rest a:hover { color: var(--accent); text-decoration: underline; }
  .minibadge {
    margin-left: auto; font-size: 10px; color: var(--ink-soft); white-space: nowrap; flex: none;
    background: var(--panel2); border: 1px solid var(--line); border-radius: 8px; padding: 1px 7px;
  }
</style>
