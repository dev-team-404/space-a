<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import './lib/theme.css';

  interface EngineSettings {
    url: string;
    key: string;
    model: string;
    source: 'store' | 'env' | 'none';
  }

  let url = $state('');
  let key = $state('');
  let model = $state('');
  let source = $state<'store' | 'env' | 'none'>('none');
  let status = $state<{ kind: 'idle' | 'ok' | 'err' | 'busy'; text: string }>({ kind: 'idle', text: '' });

  async function load() {
    try {
      const s = await invoke<EngineSettings>('engine_settings_get');
      url = s.url;
      key = s.key;
      model = s.model;
      source = s.source;
    } catch (e) {
      status = { kind: 'err', text: `설정을 불러오지 못했어요: ${e}` };
    }
  }
  load();

  async function save() {
    status = { kind: 'busy', text: '저장 중…' };
    try {
      await invoke('engine_settings_set', { url, key, model });
      await load();
      status = { kind: 'ok', text: '저장했어요. 다음 호출부터 적용됩니다.' };
    } catch (e) {
      status = { kind: 'err', text: `${e}` };
    }
  }

  async function test() {
    status = { kind: 'busy', text: '연결 확인 중…' };
    try {
      const msg = await invoke<string>('engine_test', { url, key, model });
      status = { kind: 'ok', text: msg };
    } catch (e) {
      status = { kind: 'err', text: `연결 실패: ${e}` };
    }
  }

  // --- Space A 서버 (방 방문) ---
  interface HubSettings { url: string; user: string; connected: boolean; room_id: string }
  let hub = $state<HubSettings | null>(null);
  let hubUrl = $state('');
  let hubUser = $state('');
  let hubStatus = $state<{ kind: 'idle' | 'ok' | 'err' | 'busy'; text: string }>({ kind: 'idle', text: '' });

  async function loadHub() {
    try {
      hub = await invoke<HubSettings>('hub_settings_get');
      hubUrl = hub.url;
      hubUser = hub.user;
    } catch { /* 미설정 */ }
  }
  loadHub();

  async function connectHub() {
    hubStatus = { kind: 'busy', text: '연결 중…' };
    try {
      hub = await invoke<HubSettings>('hub_connect', { url: hubUrl, user: hubUser });
      hubStatus = { kind: 'ok', text: `연결 완료 — 개인 방이 만들어졌어요 (${hub.room_id})` };
    } catch (e) {
      hubStatus = { kind: 'err', text: `${e}` };
    }
  }

  const sourceLabel = $derived(
    source === 'store' ? '설정 창에서 지정한 값 사용 중'
    : source === 'env' ? '.env 파일 값 사용 중 (아래에 저장하면 이 값을 덮어씁니다)'
    : '엔진 미설정 — 일기·채팅이 비활성 상태예요',
  );
</script>

<main>
  <h1>LLM 엔진 설정</h1>
  <p class="hint">일기·한마디·잡담·채팅이 사용할 OpenAI 호환 엔드포인트를 지정합니다.</p>
  <p class="source" data-kind={source}>{sourceLabel}</p>

  <label>
    <span>엔드포인트 URL</span>
    <input type="text" bind:value={url} placeholder="http://localhost:4444/v1" spellcheck="false" />
  </label>
  <label>
    <span>API 키 <em>(선택)</em></span>
    <input type="password" bind:value={key} placeholder="비워두면 인증 없이 호출" spellcheck="false" />
  </label>
  <label>
    <span>모델명</span>
    <input type="text" bind:value={model} placeholder="gpt-4o-mini" spellcheck="false" />
  </label>

  <div class="actions">
    <button class="primary" onclick={save} disabled={status.kind === 'busy'}>저장</button>
    <button onclick={test} disabled={status.kind === 'busy'}>연결 테스트</button>
  </div>

  {#if status.text}
    <p class="status" data-kind={status.kind}>{status.text}</p>
  {/if}

  <p class="hint">URL을 비우고 저장하면 .env(AGENT_MENTOR_ENGINE_*) 값으로 되돌아갑니다.</p>

  <hr />

  <h1>Space A 서버</h1>
  <p class="hint">방 방문·에이전트 위치를 관장하는 room-server에 연결합니다 (hub와 별개 프로세스).</p>
  {#if hub}
    <p class="source" data-kind={hub.connected ? 'store' : 'none'}>
      {hub.connected ? `연결됨 — 내 방: ${hub.room_id}` : '미연결'}
    </p>
  {/if}

  <label>
    <span>서버 URL</span>
    <input type="text" bind:value={hubUrl} placeholder="http://192.168.0.10:8001" spellcheck="false" />
  </label>
  <label>
    <span>내 이름</span>
    <input type="text" bind:value={hubUser} placeholder="예: 준녕" spellcheck="false" />
  </label>
  <div class="actions">
    <button class="primary" onclick={connectHub} disabled={hubStatus.kind === 'busy'}>연결</button>
  </div>
  {#if hubStatus.text}
    <p class="status" data-kind={hubStatus.kind}>{hubStatus.text}</p>
  {/if}
</main>

<style>
  :global(body) {
    margin: 0;
    background: var(--bg-grad);
    color: var(--ink);
    font-family: 'Segoe UI', 'Malgun Gothic', sans-serif;
  }
  main {
    box-sizing: border-box;
    min-height: 100vh;
    padding: 24px 28px;
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  h1 {
    margin: 0;
    font-size: 18px;
  }
  .hint {
    margin: 0;
    font-size: 12px;
    color: var(--ink-soft);
  }
  hr {
    width: 100%; border: none; border-top: 1px solid var(--pastel-lav); margin: 6px 0;
  }
  .source {
    margin: 0;
    font-size: 12px;
    padding: 8px 12px;
    border-radius: var(--radius-s);
    background: var(--pastel-mint);
  }
  .source[data-kind='none'] {
    background: var(--pastel-coral);
  }
  .source[data-kind='env'] {
    background: var(--pastel-cream);
  }
  label {
    display: flex;
    flex-direction: column;
    gap: 6px;
    font-size: 13px;
    font-weight: 600;
  }
  label em {
    font-weight: 400;
    font-style: normal;
    color: var(--ink-soft);
  }
  input {
    box-sizing: border-box;
    width: 100%;
    padding: 9px 12px;
    font-size: 13px;
    color: var(--ink);
    background: var(--frame-bg);
    border: 1px solid var(--pastel-lav);
    border-radius: var(--radius-s);
    outline: none;
  }
  input:focus {
    border-color: var(--accent);
  }
  .actions {
    display: flex;
    gap: 8px;
    margin-top: 4px;
  }
  button {
    padding: 9px 18px;
    font-size: 13px;
    font-weight: 600;
    color: var(--ink);
    background: var(--frame-bg);
    border: 1px solid var(--pastel-lav);
    border-radius: var(--radius-m);
    box-shadow: var(--shadow-soft);
    cursor: pointer;
  }
  button.primary {
    color: #fff;
    background: var(--accent);
    border-color: var(--accent);
  }
  button:disabled {
    opacity: 0.55;
    cursor: default;
  }
  .status {
    margin: 0;
    font-size: 12px;
    padding: 8px 12px;
    border-radius: var(--radius-s);
    background: var(--pastel-cream);
    word-break: break-all;
  }
  .status[data-kind='ok'] {
    background: var(--pastel-mint);
  }
  .status[data-kind='err'] {
    background: var(--pastel-coral);
  }
</style>
