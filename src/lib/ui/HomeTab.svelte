<script lang="ts">
  import { runScanNow, type Summary } from '../api';

  let { summary }: { summary: Summary | null } = $props();

  const fmt = (n: number | undefined) => (n ?? 0).toLocaleString();
  let scanning = $state(false);

  async function scan() {
    scanning = true;
    await runScanNow();
    setTimeout(() => (scanning = false), 3000); // 갱신 자체는 App의 scan:done 리스너가 수행
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
    마지막 스캔: {summary?.last_scan ? new Date(summary.last_scan).toLocaleString() : '아직 없음'}
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
</style>
