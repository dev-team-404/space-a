<script lang="ts">
  import { openUrl } from '@tauri-apps/plugin-opener';
  import type { LearnCardView } from '../coach-helpers';

  let { view, showLinks, coaching = null, onDismiss }: {
    view: LearnCardView;
    showLinks: boolean;
    /** 엔진 맞춤 코칭 한 줄 (최상단 1건에만) */
    coaching?: string | null;
    onDismiss: (id: string) => void;
  } = $props();

  // Tauri 웹뷰는 외부 <a> 네비게이션을 막는다 — 시스템 브라우저로 연다
  async function open(url: string) {
    try { await openUrl(url); } catch (e) { console.error('openUrl 실패:', url, e); }
  }
</script>

<article class="card">
  <header>
    <span class="badge">{view.badge}</span>
    <button class="x" title="이 항목 그만 보기" onclick={() => onDismiss(view.id)}>✕</button>
  </header>
  <h3>{view.title}</h3>
  {#if coaching}<p class="coach">🤖 {coaching}</p>{/if}
  {#if view.summary}<p class="summary">{view.summary}</p>{/if}
  {#if view.sourceUrl && showLinks}
    <a class="more" href={view.sourceUrl} onclick={(e) => { e.preventDefault(); open(view.sourceUrl!); }}>
      전문 보기 →
    </a>
  {/if}
</article>

<style>
  .card {
    background: var(--frame-bg); border-radius: var(--radius-m); box-shadow: var(--shadow-soft);
    padding: 12px 14px; border: 1px solid var(--line); border-left: 3px solid var(--accent);
    display: flex; flex-direction: column; gap: 6px;
  }
  header { display: flex; align-items: center; gap: 8px; }
  .badge {
    font-size: 10px; font-weight: 700; color: var(--accent-ink); white-space: nowrap;
    background: var(--pastel-mint); border-radius: 999px; padding: 3px 10px;
  }
  .x {
    margin-left: auto; border: none; background: none; cursor: pointer;
    color: var(--ink-soft); font-size: 12px; padding: 2px 4px; line-height: 1;
  }
  .x:hover { color: var(--accent-strong); }
  h3 { margin: 0; font-size: 14px; color: var(--ink); font-weight: 700; line-height: 1.4; }
  .coach { margin: 0; font-size: 12.5px; color: var(--lav); line-height: 1.6; }
  .summary { margin: 0; font-size: 12px; color: var(--ink-soft); line-height: 1.7; white-space: pre-line; }
  /* 문법 B는 전문 링크가 주 CTA — 문법 A의 각주 링크와 반대다 (스펙 §4.2) */
  .more { font-size: 12px; color: var(--accent-strong); text-decoration: none; font-weight: 600; width: fit-content; }
  .more:hover { text-decoration: underline; }
</style>
