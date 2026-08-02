<script lang="ts">
  import { openUrl } from '@tauri-apps/plugin-opener';
  import type { Snippet } from 'svelte';
  import type { LogCardView } from '../coach-helpers';

  let { view, showLinks, actions, extra, footer }: {
    view: LogCardView;
    showLinks: boolean;
    /** 행동·처분 버튼 — 핸들러가 CoachTab에 있어 주입받는다 */
    actions?: Snippet;
    /** 포함 세션 목록 등 finding 전용 부가 영역 (행동 줄 위) */
    extra?: Snippet;
    /** 「원본 데이터」 토글 등 카드 맨 아래 부가 영역 */
    footer?: Snippet;
  } = $props();

  // Tauri 웹뷰는 외부 <a> 네비게이션을 막는다 — 시스템 브라우저로 연다
  async function open(url: string) {
    try { await openUrl(url); } catch (e) { console.error('openUrl 실패:', url, e); }
  }
</script>

<article class="card" class:warn={view.icon === '⚠'} data-key={view.key}>
  <header>
    <span class="title">{view.icon} {view.title}</span>
    {#if view.chip}<span class="chip">{view.chip}</span>{/if}
  </header>
  {#if view.evidence}<p class="evidence">📊 {view.evidence}</p>{/if}
  {#if view.principle}<p class="principle">{view.principle}</p>{/if}
  {#if view.reason}<p class="judgment">🧭 코치 판정: {view.reason}</p>{/if}
  {@render extra?.()}
  {#if view.action}<p class="how">➜ {view.action}</p>{/if}
  {#if actions}<div class="actions">{@render actions()}</div>{/if}
  {@render footer?.()}
  {#if view.sourceUrl && showLinks}
    <a class="more" href={view.sourceUrl} onclick={(e) => { e.preventDefault(); open(view.sourceUrl!); }}>왜 그런지 더 보기 →</a>
  {/if}
</article>

<style>
  .card {
    background: var(--frame-bg); border-radius: var(--radius-m); box-shadow: var(--shadow-soft);
    padding: 12px 14px; border-left: 4px solid var(--pastel-mint);
  }
  .card.warn { border-left-color: var(--pastel-coral); }
  header { display: flex; justify-content: space-between; gap: 8px; align-items: baseline; }
  .title { font-weight: 600; }
  .chip {
    color: var(--ink-soft); font-size: 11px; white-space: nowrap; flex: none;
    background: var(--panel2); border: 1px solid var(--line); border-radius: 8px; padding: 1px 7px;
  }
  .evidence { margin: 6px 0 2px; font-size: 12px; color: var(--ink); line-height: 1.6; white-space: pre-line; }
  .principle { margin: 2px 0; font-size: 12px; color: var(--ink-soft); line-height: 1.7; white-space: pre-line; }
  .judgment { margin: 2px 0 6px; font-size: 12px; color: var(--accent-strong); }
  .how { margin: 4px 0 8px; font-size: 13px; white-space: pre-line; }
  .actions { display: flex; gap: 6px; flex-wrap: wrap; }
  /* 버튼은 CoachTab의 snippet이 그리므로 이 컴포넌트의 스코프 클래스가 붙지 않는다.
     :global()로 감싸지 않으면 Svelte가 미사용으로 판단해 규칙을 지운다. */
  .actions :global(button) {
    border: none; cursor: pointer; font: inherit; font-size: 12px;
    background: var(--pastel-lav); color: var(--ink);
    border-radius: var(--radius-s); padding: 5px 10px;
  }
  .actions :global(.cmd) { background: var(--pastel-cream); font-family: Consolas, monospace; }
  .actions :global(.skillify) { background: var(--pastel-mint); font-weight: 600; }
  /* 각주 크기 — 주 CTA는 행동 버튼이고 링크는 보조다 (스펙 §4.1) */
  .more { display: block; margin-top: 8px; font-size: 11px; color: var(--ink-soft); text-decoration: none; }
  .more:hover { color: var(--accent-strong); text-decoration: underline; }
</style>
