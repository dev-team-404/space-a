<script lang="ts">
  import { openUrl } from '@tauri-apps/plugin-opener';
  import type { LearnCardView } from '../coach-helpers';

  let { view, showLinks, pinned = false, onAck, onDismiss }: {
    view: LearnCardView;
    showLinks: boolean;
    /** 최상단 고정 슬롯으로 그린다 (유효 기한 + 미확인, 최대 1건 — 스펙 §6.4) */
    pinned?: boolean;
    /** 고정 슬롯의 [확인] — 누르면 즉시 일반 소식 카드로 강등된다 */
    onAck?: (id: string) => void;
    onDismiss: (id: string) => void;
  } = $props();

  // Tauri 웹뷰는 외부 <a> 네비게이션을 막는다 — 시스템 브라우저로 연다
  async function open(url: string) {
    try { await openUrl(url); } catch (e) { console.error('openUrl 실패:', url, e); }
  }

  const deadlineChip = $derived(view.deadline ? `⏳ ${view.deadline.slice(5)}까지` : null);
</script>

<!-- data-key: 홈 알림·말풍선 딥링크가 이 카드로 스크롤한다 (LogCard와 같은 규약) -->
<article class="card" class:pinned data-key={view.id}>
  <header>
    <span class="badge">{view.badge}</span>
    {#if deadlineChip}<span class="deadline">{deadlineChip}</span>{/if}
    {#if pinned && onAck}
      <button class="ack" onclick={() => onAck(view.id)}>확인</button>
    {/if}
    <button class="x" title="이 항목 그만 보기" onclick={() => onDismiss(view.id)}>✕</button>
  </header>
  <h3>{view.title}</h3>
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
  /* 고정 슬롯은 기한이 있는 한 건뿐 — 테두리로만 구분하고 문법은 그대로 둔다 (스펙 §6.4) */
  .card.pinned { border: 1px solid var(--accent); border-left-width: 3px; }
  header { display: flex; align-items: center; gap: 8px; }
  .badge {
    font-size: 10px; font-weight: 700; color: var(--accent-ink); white-space: nowrap;
    background: var(--pastel-mint); border-radius: 999px; padding: 3px 10px;
  }
  .deadline {
    font-size: 10px; font-weight: 700; color: var(--accent-strong); white-space: nowrap;
    background: var(--surface-inset); border-radius: 999px; padding: 3px 10px;
  }
  .ack {
    margin-left: auto; border: none; cursor: pointer; font: inherit; font-size: 11px;
    background: var(--pastel-mint); color: var(--ink); border-radius: var(--radius-s); padding: 3px 10px;
  }
  .x {
    margin-left: auto; border: none; background: none; cursor: pointer;
    color: var(--ink-soft); font-size: 12px; padding: 2px 4px; line-height: 1;
  }
  /* [확인]이 있으면 그쪽이 오른쪽 끝을 잡는다 — ✕는 바로 옆에 붙는다 */
  .ack + .x { margin-left: 0; }
  .x:hover { color: var(--accent-strong); }
  h3 { margin: 0; font-size: 14px; color: var(--ink); font-weight: 700; line-height: 1.4; }
  .summary { margin: 0; font-size: 12px; color: var(--ink-soft); line-height: 1.7; white-space: pre-line; }
  /* 문법 B는 전문 링크가 주 CTA — 문법 A의 각주 링크와 반대다 (스펙 §4.2) */
  .more { font-size: 12px; color: var(--accent-strong); text-decoration: none; font-weight: 600; width: fit-content; }
  .more:hover { text-decoration: underline; }
</style>
