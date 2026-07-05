<script lang="ts">
  import { onScanDone, runScanNow, type Summary } from '../api';

  let { summary, onGotoCoach }: { summary: Summary | null; onGotoCoach: (k: string) => void } = $props();
  void onGotoCoach;

  const fmt = (n: number | undefined) => (n ?? 0).toLocaleString();
  let scanning = $state(false);

  $effect(() => {
    const p = onScanDone(() => (scanning = false));
    return () => { p.then((u) => u()); };
  });

  async function scan() {
    scanning = true;
    try {
      await runScanNow(); // 완료 신호(scanning 해제·데이터 갱신)는 scan:done 이벤트가 담당
    } catch {
      scanning = false;
    }
  }
</script>

<section class="home">
  <h2>{summary?.date ?? ''} 오늘 요약</h2>
  <div class="cards">
    <div class="card">세션 <b>{fmt(summary?.session_count)}</b></div>
    <div class="card">입력 <b>{fmt(summary?.tok_input)}</b> tok</div>
    <div class="card">출력 <b>{fmt(summary?.tok_output)}</b> tok</div>
    <div class="card">캐시 읽기 <b>{fmt(summary?.tok_cache_read)}</b> tok</div>
    <div class="card save">절약 가능 <b>{fmt(summary?.est_tokens_saved_total)}</b> tok</div>
  </div>
  <footer class="status">
    {#if scanning}
      <span class="scanning">스캔 중…</span>
    {:else if summary?.last_scan}
      마지막 스캔: {new Date(summary.last_scan).toLocaleString()}
    {:else}
      첫 수집 진행 중… (트랜스크립트 양에 따라 몇 분 걸릴 수 있어요)
    {/if}
    <button onclick={scan} disabled={scanning}>{scanning ? '스캔 중…' : '지금 스캔'}</button>
  </footer>
</section>

<style>
  .home { padding: 16px; display: flex; flex-direction: column; flex: 1; }
  .cards { display: flex; flex-wrap: wrap; gap: 10px; }
  .card {
    border: 3px solid #33325a; background: #fffdf5; padding: 12px 16px;
    box-shadow: 4px 4px 0 #c9c3dd;
  }
  .card.save { background: #fdf1c7; }
  .status { margin-top: auto; display: flex; justify-content: space-between; align-items: center; font-size: 12px; }
  .status button { border: 3px solid #33325a; background: #d9d4e8; font: inherit; padding: 4px 10px; cursor: pointer; }
  .scanning { animation: blink 1.2s ease-in-out infinite; }
  @keyframes blink { 50% { opacity: 0.35; } }
</style>
