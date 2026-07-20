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
  interface HubSettings { url: string; user: string; api_key: string; connected: boolean; life_id: string }
  let hub = $state<HubSettings | null>(null);
  let hubUrl = $state('');
  let hubUser = $state('');
  let hubApiKey = $state('');
  let hubStatus = $state<{ kind: 'idle' | 'ok' | 'err' | 'busy'; text: string }>({ kind: 'idle', text: '' });

  async function loadHub() {
    try {
      hub = await invoke<HubSettings>('hub_settings_get');
      hubUrl = hub.url;
      hubUser = hub.user;
      hubApiKey = hub.api_key;
    } catch { /* 미설정 */ }
  }
  loadHub();

  async function connectHub() {
    hubStatus = { kind: 'busy', text: '연결 중…' };
    try {
      hub = await invoke<HubSettings>('hub_connect', { url: hubUrl, user: hubUser, apiKey: hubApiKey });
      hubStatus = { kind: 'ok', text: `연결 완료 — 개인 방이 만들어졌어요 (${hub.life_id})` };
    } catch (e) {
      hubStatus = { kind: 'err', text: `${e}` };
    }
  }

  const sourceLabel = $derived(
    source === 'store' ? '설정 창에서 지정한 값 사용 중'
    : source === 'env' ? '.env 파일 값 사용 중 (아래에 저장하면 이 값을 덮어씁니다)'
    : '엔진 미설정 — 일기·채팅이 비활성 상태예요',
  );

  // --- 캐릭터 이미지 모델 (텍스트 엔진과 분리) ---
  // 사내 LLM(LM Studio 등)은 이미지를 생성하지 못하므로, 캐릭터 그림용 엔드포인트를 따로 둔다.
  interface ImageSettings { url: string; key: string; model: string; source: 'store' | 'env' | 'none' }

  let imgUrl = $state('');
  let imgKey = $state('');
  let imgModel = $state('');
  let imgSource = $state<'store' | 'env' | 'none'>('none');
  let imgStatus = $state<{ kind: 'idle' | 'ok' | 'err' | 'busy'; text: string }>({ kind: 'idle', text: '' });

  async function loadImage() {
    try {
      const s = await invoke<ImageSettings>('image_settings_get');
      imgUrl = s.url; imgKey = s.key; imgModel = s.model; imgSource = s.source;
    } catch (e) {
      imgStatus = { kind: 'err', text: `이미지 설정을 불러오지 못했어요: ${e}` };
    }
  }
  loadImage();

  async function saveImage() {
    imgStatus = { kind: 'busy', text: '저장 중…' };
    try {
      await invoke('image_settings_set', { url: imgUrl, key: imgKey, model: imgModel });
      await loadImage();
      imgStatus = { kind: 'ok', text: '저장했어요.' };
    } catch (e) {
      imgStatus = { kind: 'err', text: `${e}` };
    }
  }

  async function regenerate() {
    imgStatus = { kind: 'busy', text: '캐릭터 그리는 중… 수십 초 걸릴 수 있어요' };
    try {
      await invoke('regenerate_sprite');
      imgStatus = { kind: 'ok', text: '새 캐릭터로 바뀌었어요 — 마스코트를 확인해보세요!' };
    } catch (e) {
      imgStatus = { kind: 'err', text: `생성 실패: ${e}` };
    }
  }

  const imgSourceLabel = $derived(
    imgSource === 'store'
      ? '설정창 값 사용 중'
      : imgSource === 'env'
        ? '.env 값 사용 중'
        : '미설정 — 캐릭터가 기본 그림으로 표시됩니다'
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

  <h1>캐릭터 이미지</h1>
  <p class="hint">
    마스코트 캐릭터를 그릴 <b>이미지 생성 모델</b>입니다. 사내 LLM은 그림을 못 그리므로 위 텍스트 엔진과
    따로 지정합니다. 사람마다 한 번 생성해 캐시하므로, 이후에는 호출하지 않습니다.
  </p>
  <p class="source" data-kind={imgSource}>{imgSourceLabel}</p>

  <label>
    <span>엔드포인트 URL</span>
    <input type="text" bind:value={imgUrl} placeholder="https://openrouter.ai/api/v1" spellcheck="false" />
  </label>
  <label>
    <span>API 키</span>
    <input type="password" bind:value={imgKey} placeholder="sk-or-..." spellcheck="false" />
  </label>
  <label>
    <span>이미지 모델</span>
    <input type="text" bind:value={imgModel} placeholder="google/gemini-2.5-flash-image" spellcheck="false" />
  </label>

  <div class="actions">
    <button class="primary" onclick={saveImage} disabled={imgStatus.kind === 'busy'}>저장</button>
    <button onclick={regenerate} disabled={imgStatus.kind === 'busy'}>캐릭터 재생성</button>
  </div>

  {#if imgStatus.text}
    <p class="status" data-kind={imgStatus.kind}>{imgStatus.text}</p>
  {/if}

  <hr />

  <h1>Space A 서버</h1>
  <p class="hint">방 방문·에이전트 위치를 관장하는 life-server에 연결합니다 (hub와 별개 프로세스).</p>
  {#if hub}
    <p class="source" data-kind={hub.connected ? 'store' : 'none'}>
      {hub.connected ? `연결됨 — 내 방: ${hub.life_id}` : '미연결'}
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
  <label>
    <span>API 키 <em>(선택)</em></span>
    <input type="password" bind:value={hubApiKey} placeholder="비워두면 인증 없이 (관문 켜진 서버만 필요)" spellcheck="false" />
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
