<script lang="ts">
  import { getVersion } from '@tauri-apps/api/app';
  import { runCheck, updateStore } from '../update-store.svelte';

  // 설치된 앱 버전 — tauri.conf.json의 version이 원본. 업데이트 적용 여부 확인용.
  let version = $state('…');
  getVersion().then((v) => { version = v; }).catch(() => { version = '알 수 없음'; });

  const checking = $derived(updateStore.phase.kind === 'checking');
</script>

<footer class="appinfo">
  <span>Agent Mentor <b>{version}</b></span>
  <!-- 결과는 창 상단 업데이트 배너에 뜬다 (트레이 "업데이트 확인"과 같은 경로) -->
  <button onclick={() => runCheck(true)} disabled={checking}>
    {checking ? '확인 중…' : '업데이트 확인'}
  </button>
</footer>

<style>
  .appinfo{
    display:flex;align-items:center;justify-content:space-between;gap:10px;
    margin-top:24px;padding-top:14px;border-top:1px solid var(--line);
    font-size:12px;color:var(--text-soft);
  }
  .appinfo b{color:var(--accent-strong);font-weight:700}
  .appinfo button{
    border:1px solid var(--line);border-radius:99px;padding:5px 12px;
    background:transparent;color:var(--text);cursor:pointer;font:inherit;
  }
  .appinfo button:hover:not(:disabled){background:var(--surface-inset)}
  .appinfo button:disabled{opacity:.55;cursor:default}
</style>
