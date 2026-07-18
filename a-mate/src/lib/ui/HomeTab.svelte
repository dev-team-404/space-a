<script lang="ts">
  import {
    getWeekSummary, listFindings, listContent, onContentReady,
    onScanDone, onScanProgress, onSettingsChanged, runScanNow,
    type CoachFinding, type ContentItem, type DayStat, type ScanProgress, type Summary,
  } from '../api';
  import { loadNotices, type Notice, type NoticeDest } from '../notices';
  import WeekTrend from './home/WeekTrend.svelte';
  import ModelMix from './home/ModelMix.svelte';
  import SaveTop3 from './home/SaveTop3.svelte';
  import NoticeLog from './home/NoticeLog.svelte';
  import TipCard from './home/TipCard.svelte';
  import MiniRoom from './MiniRoom.svelte';
  import RoomView from './RoomView.svelte';
  import { hubSettingsGet } from '../api';

  let { summary, visiting = false, onGotoCoach, onGotoNotice }: {
    summary: Summary | null;
    visiting?: boolean;
    onGotoCoach: (k: string) => void;
    onGotoNotice: (dest: NoticeDest) => void;
  } = $props();

  const fmt = (n: number | undefined) => (n ?? 0).toLocaleString();
  let scanning = $state(false);
  let progress = $state<ScanProgress | null>(null);
  let days = $state<DayStat[]>([]);
  let findings = $state<CoachFinding[]>([]);
  let notices = $state<Notice[]>([]);
  let tips = $state<ContentItem[]>([]);
  const topAdvice = $derived(findings.length > 0 ? findings[0].suggested_action : null);
  // 방 서버 연결 시 격자 방(RoomView), 미연결 시 기존 장식 방(MiniRoom) — 원기능 보존
  // 설정 창에서 연결하는 순간 바뀌도록 settings:changed와 창 포커스에 반응한다
  let hubConnected = $state(false);
  const interiorPreview = import.meta.env.DEV && new URLSearchParams(location.search).has('interiorPreview');
  const checkHub = () => hubSettingsGet().then((h) => (hubConnected = h.connected)).catch(() => {});
  checkHub();

  async function load() {
    [days, findings, tips] = await Promise.all([
      getWeekSummary().catch(() => [] as DayStat[]),
      listFindings(false).catch(() => [] as CoachFinding[]),
      listContent(false).catch(() => [] as ContentItem[]),
    ]);
    notices = loadNotices();
  }
  load();

  $effect(() => {
    const subs = [
      onScanProgress((p) => { scanning = true; progress = p; }),
      onScanDone(() => { scanning = false; progress = null; load(); }),
      onContentReady((rows) => { tips = rows; }),
      onSettingsChanged(() => checkHub()),
    ];
    window.addEventListener('focus', checkHub);
    return () => {
      subs.forEach((s) => s.then((u) => u()));
      window.removeEventListener('focus', checkHub);
    };
  });

  // 팁 하나 닫으면 목록에서 즉시 제거(백엔드는 다음 스캔에 쿨다운 반영)
  function onTipDismissed(id: string) {
    tips = tips.filter((t) => t.id !== id);
  }

  async function scan() {
    scanning = true;
    try {
      await runScanNow(); // 완료 신호는 scan:done 이벤트가 담당
    } catch {
      scanning = false;
    }
  }
</script>

<section class="home">
  {#if hubConnected || interiorPreview}
    <RoomView />
  {:else}
    <MiniRoom advice={topAdvice} />
  {/if}

  <!-- 아래는 전부 내 로컬 데이터 — 남의 방을 보는 동안엔 숨긴다 (남의 것으로 오독 방지) -->
  {#if !visiting}
  <div class="strip">
    <span>세션 <b>{fmt(summary?.session_count)}</b></span>
    <span>입력 <b>{fmt(summary?.tok_input)}</b></span>
    <span>출력 <b>{fmt(summary?.tok_output)}</b></span>
    <span class="save">절약 가능 <b>{fmt(summary?.est_tokens_saved_total)}</b> tok</span>
  </div>

  <TipCard items={tips} onDismissed={onTipDismissed} />

  <div class="grid">
    <WeekTrend {days} />
    <ModelMix />
    <SaveTop3 {findings} onGoto={onGotoCoach} />
    <NoticeLog {notices} onGoto={onGotoNotice} />
  </div>

  <footer class="status">
    {#if scanning}
      <span class="scan-live">
        <span class="scanning">스캔 중…</span>
        {#if progress && progress.total > 0}
          <span class="bar"><span class="fill" style="width: {Math.min(100, Math.round((progress.done / progress.total) * 100))}%"></span></span>
          <span class="pct">{progress.done}/{progress.total}</span>
        {/if}
      </span>
    {:else if summary?.last_scan}
      <span>마지막 스캔: {new Date(summary.last_scan).toLocaleString()}</span>
    {:else}
      <span>첫 수집 진행 중… (트랜스크립트 양에 따라 몇 분 걸릴 수 있어요)</span>
    {/if}
    <button onclick={scan} disabled={scanning}>{scanning ? '스캔 중…' : '지금 스캔'}</button>
  </footer>
  {/if}
</section>

<style>
  /* content가 스크롤 컨테이너이므로 홈 자체는 내용 높이를 보존한다.
     flex:1 + 기본 shrink 조합은 방이 커질 때 아래 카드가 방 위로 겹쳐 보일 수 있다. */
  .home {
    padding: 14px 16px; display: flex; flex-direction: column; gap: 12px;
    flex: 0 0 auto; min-height: 100%; box-sizing: border-box; position: relative;
  }
  .strip {
    display: flex; gap: 16px; flex-wrap: wrap; font-size: 12px; color: var(--ink-soft);
    background: var(--frame-bg); border-radius: var(--radius-m); box-shadow: var(--shadow-soft);
    padding: 9px 14px;
  }
  .strip b { color: var(--ink); }
  .strip .save b { color: var(--accent); }
  .grid { display: grid; grid-template-columns: 1fr 1fr; gap: 12px; position: relative; flex: 0 0 auto; }
  /* grid 아이템 기본 min-width:auto가 긴 top3(nowrap)에 밀려 컬럼을 늘리는 것 방지 — 1fr 고정·ellipsis 복구 */
  .grid > :global(*) { min-width: 0; }
  .status {
    margin-top: auto; display: flex; justify-content: space-between; align-items: center;
    font-size: 12px; color: var(--ink-soft);
  }
  .status button {
    border: none; cursor: pointer; font: inherit; font-size: 12px;
    background: var(--pastel-lav); color: var(--ink);
    border-radius: var(--radius-s); padding: 6px 12px;
  }
  .status button:disabled { opacity: 0.6; cursor: default; }
  .scanning { animation: blink 1.2s ease-in-out infinite; }
  @keyframes blink { 50% { opacity: 0.35; } }
  .scan-live { display: flex; align-items: center; gap: 8px; }
  .bar {
    width: 140px; height: 6px; border-radius: 999px;
    background: var(--pastel-lav); overflow: hidden; display: inline-block;
  }
  .fill { display: block; height: 100%; background: var(--accent); border-radius: 999px; transition: width 0.2s ease; }
  .pct { font-size: 11px; color: var(--ink-soft); }
</style>
