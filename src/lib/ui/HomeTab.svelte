<script lang="ts">
  import {
    getModelMix, getWeekSummary, listFindings, onScanDone, onScanProgress, runScanNow,
    type CoachFinding, type DayStat, type ModelMixEntry, type ScanProgress, type Summary,
  } from '../api';
  import { loadNotices, type Notice } from '../notices';
  import WeekTrend from './home/WeekTrend.svelte';
  import ModelMix from './home/ModelMix.svelte';
  import SaveTop3 from './home/SaveTop3.svelte';
  import NoticeLog from './home/NoticeLog.svelte';
  import MiniRoom from './MiniRoom.svelte';

  let { summary, onGotoCoach }: { summary: Summary | null; onGotoCoach: (k: string) => void } = $props();

  const fmt = (n: number | undefined) => (n ?? 0).toLocaleString();
  let scanning = $state(false);
  let progress = $state<ScanProgress | null>(null);
  let days = $state<DayStat[]>([]);
  let mix = $state<ModelMixEntry[]>([]);
  let findings = $state<CoachFinding[]>([]);
  let notices = $state<Notice[]>([]);
  const topAdvice = $derived(findings.length > 0 ? findings[0].suggested_action : null);

  async function load() {
    [days, mix, findings] = await Promise.all([
      getWeekSummary().catch(() => [] as DayStat[]),
      getModelMix().catch(() => [] as ModelMixEntry[]),
      listFindings(false).catch(() => [] as CoachFinding[]),
    ]);
    notices = loadNotices();
  }
  load();

  $effect(() => {
    const subs = [
      onScanProgress((p) => { scanning = true; progress = p; }),
      onScanDone(() => { scanning = false; progress = null; load(); }),
    ];
    return () => { subs.forEach((s) => s.then((u) => u())); };
  });

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
  <div class="strip">
    <span>세션 <b>{fmt(summary?.session_count)}</b></span>
    <span>입력 <b>{fmt(summary?.tok_input)}</b></span>
    <span>출력 <b>{fmt(summary?.tok_output)}</b></span>
    <span class="save">절약 가능 <b>{fmt(summary?.est_tokens_saved_total)}</b> tok</span>
  </div>

  <div class="grid">
    <WeekTrend {days} />
    <ModelMix {mix} />
    <SaveTop3 {findings} onGoto={onGotoCoach} />
    <NoticeLog {notices} />
  </div>

  <MiniRoom advice={topAdvice} />

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
</section>

<style>
  .home { padding: 14px 16px; display: flex; flex-direction: column; gap: 12px; flex: 1; }
  .strip {
    display: flex; gap: 16px; flex-wrap: wrap; font-size: 12px; color: var(--ink-soft);
    background: var(--frame-bg); border-radius: var(--radius-m); box-shadow: var(--shadow-soft);
    padding: 9px 14px;
  }
  .strip b { color: var(--ink); }
  .strip .save b { color: var(--accent); }
  .grid { display: grid; grid-template-columns: 1fr 1fr; gap: 12px; }
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
