<script lang="ts">
  import {
    engineSettingsGet, engineSettingsSet, engineTest,
    hubConnect, hubDisconnect, hubSettingsGet,
    imageSettingsGet, imageSettingsSet, regenerateSprite,
    type EngineSettings, type HubSettings, type ImageSettings,
  } from '../../api';
  import { IDLE, busy, err, ok, type Status } from './status';
  import StatusLine from './StatusLine.svelte';

  // --- Life Server (방 방문·에이전트 위치) ---
  let hub = $state<HubSettings | null>(null);
  let hubUrl = $state(''), hubApiKey = $state('');
  let hubStatus = $state<Status>(IDLE);
  async function loadHub(){ try { hub = await hubSettingsGet(); hubUrl = hub.url; hubApiKey = hub.api_key; } catch { /* 미설정 */ } }
  loadHub();
  async function connectHub(){
    hubStatus = busy('연결 중…');
    try { hub = await hubConnect(hubUrl, hubApiKey); hubStatus = ok(`연결 완료 — 개인 방이 만들어졌어요 (${hub.life_id})`); }
    catch(e){ hubStatus = err(e); }
  }
  async function disconnectHub(){
    hubStatus = busy('연결 종료 중…');
    try { hub = await hubDisconnect(); hubStatus = ok('Life 서버 연결을 종료했어요. 서버의 방과 공개 데이터는 유지됩니다.'); }
    catch(e){ hubStatus = err(e); }
  }

  // --- 텍스트 LLM 엔진 ---
  let eng = $state<EngineSettings>({ url:'', key:'', model:'', source:'none' });
  let engStatus = $state<Status>(IDLE);
  async function loadEngine(){
    try { eng = await engineSettingsGet(); } catch(e){ engStatus = err(`설정을 불러오지 못했어요: ${e}`); }
  }
  loadEngine();
  async function saveEngine(){
    engStatus = busy('저장 중…');
    try { await engineSettingsSet(eng.url, eng.key, eng.model); await loadEngine(); engStatus = ok('저장했어요. 다음 호출부터 적용됩니다.'); }
    catch(e){ engStatus = err(e); }
  }
  async function testEngine(){
    engStatus = busy('연결 확인 중…');
    try { engStatus = ok(await engineTest(eng.url, eng.key, eng.model)); }
    catch(e){ engStatus = err(`연결 실패: ${e}`); }
  }
  const engSourceLabel = $derived(
    eng.source === 'store' ? '설정 탭에서 지정한 값 사용 중'
    : eng.source === 'env' ? '.env 파일 값 사용 중 (아래에 저장하면 이 값을 덮어씁니다)'
    : '엔진 미설정 — 일기·채팅이 비활성 상태예요',
  );

  // --- 캐릭터 이미지 모델 (사내 LLM은 그림을 못 그려 텍스트 엔진과 분리) ---
  let img = $state<ImageSettings>({ url:'', key:'', model:'', source:'none' });
  let imgStatus = $state<Status>(IDLE);
  async function loadImage(){
    try { img = await imageSettingsGet(); } catch(e){ imgStatus = err(`이미지 설정을 불러오지 못했어요: ${e}`); }
  }
  loadImage();
  async function saveImage(){
    imgStatus = busy('저장 중…');
    try { await imageSettingsSet(img.url, img.key, img.model); await loadImage(); imgStatus = ok('저장했어요.'); }
    catch(e){ imgStatus = err(e); }
  }
  async function regenerate(){
    imgStatus = busy('캐릭터 그리는 중… 수십 초 걸릴 수 있어요');
    try { await regenerateSprite(); imgStatus = ok('새 캐릭터로 바뀌었어요 — 마스코트를 확인해보세요!'); }
    catch(e){ imgStatus = err(`생성 실패: ${e}`); }
  }
  const imgSourceLabel = $derived(
    img.source === 'store' ? '설정 탭에서 지정한 값 사용 중'
    : img.source === 'env' ? '.env 값 사용 중'
    : '미설정 — 캐릭터가 기본 그림으로 표시됩니다',
  );
</script>

<section>
  <h2>Life Server</h2>
  <p class="hint">방 방문·에이전트 위치를 관장하는 life-server에 연결합니다 (hub와 별개 프로세스).</p>
  {#if hub}
    <p class="source" data-kind={hub.connected ? 'store' : 'none'}>
      {hub.connected ? `연결됨 — 내 방: ${hub.life_id}` : '미연결'}
    </p>
  {/if}
  <div class="fields">
    <label class="field"><span>서버 URL</span>
      <input type="text" bind:value={hubUrl} placeholder="http://192.168.0.10:8001" spellcheck="false"/></label>
    <label class="field"><span>API 키 <em>(선택)</em></span>
      <input type="password" bind:value={hubApiKey} placeholder="비워두면 인증 없이 (관문 켜진 서버만 필요)" spellcheck="false"/></label>
  </div>
  <div class="actions">
    <button class="primary" onclick={connectHub} disabled={hubStatus.kind==='busy'}>연결</button>
    {#if hub?.connected}<button onclick={disconnectHub} disabled={hubStatus.kind==='busy'}>연결 종료</button>{/if}
  </div>
  <StatusLine status={hubStatus}/>
</section>

<section>
  <h2>LLM 엔진</h2>
  <p class="hint">일기·한마디·잡담·채팅이 사용할 OpenAI 호환 엔드포인트를 지정합니다.</p>
  <p class="source" data-kind={eng.source}>{engSourceLabel}</p>
  <div class="fields">
    <label class="field"><span>엔드포인트 URL</span>
      <input type="text" bind:value={eng.url} placeholder="http://localhost:4444/v1" spellcheck="false"/></label>
    <label class="field"><span>API 키 <em>(선택)</em></span>
      <input type="password" bind:value={eng.key} placeholder="비워두면 인증 없이 호출" spellcheck="false"/></label>
    <label class="field"><span>모델명</span>
      <input type="text" bind:value={eng.model} placeholder="gpt-4o-mini" spellcheck="false"/></label>
  </div>
  <div class="actions">
    <button class="primary" onclick={saveEngine} disabled={engStatus.kind==='busy'}>저장</button>
    <button onclick={testEngine} disabled={engStatus.kind==='busy'}>연결 테스트</button>
  </div>
  <StatusLine status={engStatus}/>
  <p class="hint2">URL을 비우고 저장하면 .env(AGENT_MENTOR_ENGINE_*) 값으로 되돌아갑니다.</p>
</section>

<section>
  <h2>캐릭터 이미지</h2>
  <p class="hint">마스코트 캐릭터를 그릴 이미지 생성 모델입니다. 사내 LLM은 그림을 못 그리므로 위 텍스트 엔진과 따로 지정합니다. 사람마다 한 번 생성해 캐시하므로 이후에는 호출하지 않습니다.</p>
  <p class="source" data-kind={img.source}>{imgSourceLabel}</p>
  <div class="fields">
    <label class="field"><span>엔드포인트 URL</span>
      <input type="text" bind:value={img.url} placeholder="https://openrouter.ai/api/v1" spellcheck="false"/></label>
    <label class="field"><span>API 키</span>
      <input type="password" bind:value={img.key} placeholder="sk-or-..." spellcheck="false"/></label>
    <label class="field"><span>이미지 모델</span>
      <input type="text" bind:value={img.model} placeholder="google/gemini-2.5-flash-image" spellcheck="false"/></label>
  </div>
  <div class="actions">
    <button class="primary" onclick={saveImage} disabled={imgStatus.kind==='busy'}>저장</button>
    <button onclick={regenerate} disabled={imgStatus.kind==='busy'}>캐릭터 재생성</button>
  </div>
  <StatusLine status={imgStatus}/>
</section>

<style>
  .source{margin:8px 0 0;font-size:12px;padding:7px 11px;border-radius:8px;background:var(--lav-surface);color:var(--lav-ink)}
  .source[data-kind=none]{background:var(--surface-inset);color:var(--danger)}
  .source[data-kind=env]{background:var(--cream);color:var(--cream-ink)}
  .fields{display:grid;grid-template-columns:repeat(2,1fr);gap:10px;margin-top:12px}
  .field{display:flex;flex-direction:column;gap:4px}
  .field>span{font-size:12px;color:var(--text-soft)}
  .field em{font-style:normal;opacity:.7}
  .field input{border:1px solid var(--line);border-radius:8px;padding:8px 10px;background:var(--surface-inset);color:var(--text);font:inherit}
  .actions{display:flex;gap:8px;margin-top:12px}
  .actions button{border:0;border-radius:99px;padding:7px 14px;background:var(--lav-surface);color:var(--lav-ink);cursor:pointer;font:inherit}
  .actions button.primary{background:var(--accent);color:var(--accent-ink);font-weight:700}
  .actions button:disabled{opacity:.55;cursor:default}
  .hint2{color:var(--text-soft);font-size:11px;margin:8px 0 0}
</style>
