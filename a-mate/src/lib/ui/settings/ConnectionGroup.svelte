<script lang="ts">
  import {
    engineSettingsGet, engineSettingsSet, engineTest,
    hubConnect, hubDisconnect, hubSettingsGet,
    imageSettingsGet, imageSettingsSet, imageTest,
    getSettings, setSetting,
    knowledgeHubSettingsGet, knowledgeHubSettingsSet, knowledgeHubShareSet,
    type EngineSettings, type HubSettings, type ImageApi, type ImageSettings,
    type KnowledgeHubSettings,
  } from '../../api';
  import { LENS_OFF, LENS_URL_PLACEHOLDER } from '../../lens';
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

  // 사내 게이트웨이가 요구하는 신원 헤더 예시 — 입력 칸 안내용(저장값 아님).
  const HEADER_PLACEHOLDER = 'x-user-id: abc\nx-dept-name: s/w개발팀\nx-service-id: service-a';

  // 값에 비ASCII가 있어 퍼센트 인코딩되어 나갈 헤더 이름들. 백엔드(http_headers.rs)의
  // 판정과 같은 규칙이며, 저장 전에도 알려주려고 프런트에서 한 번 더 본다.
  function nonAsciiNames(raw: string): string[] {
    return raw.split('\n')
      .map((l) => l.trim())
      .filter((l) => l && !l.startsWith('#'))
      .map((l) => l.split(/:(.*)/s))
      .filter(([n, v]) => n?.trim() && v?.trim() && /[^\x20-\x7e\t]/.test(v))
      .map(([n]) => n.trim().replace(/^["']|["']$/g, ''));
  }

  // --- 텍스트 LLM 엔진 ---
  let eng = $state<EngineSettings>({ url:'', key:'', model:'', headers:'', source:'none' });
  let engStatus = $state<Status>(IDLE);
  async function loadEngine(){
    try { eng = await engineSettingsGet(); } catch(e){ engStatus = err(`설정을 불러오지 못했어요: ${e}`); }
  }
  loadEngine();
  async function saveEngine(){
    engStatus = busy('저장 중…');
    try { await engineSettingsSet(eng.url, eng.key, eng.model, eng.headers); await loadEngine(); engStatus = ok('저장했어요. 다음 호출부터 적용됩니다.'); }
    catch(e){ engStatus = err(e); }
  }
  async function testEngine(){
    engStatus = busy('연결 확인 중…');
    try { engStatus = ok(await engineTest(eng.url, eng.key, eng.model, eng.headers)); }
    catch(e){ engStatus = err(`연결 실패: ${e}`); }
  }
  const engSourceLabel = $derived(
    eng.source === 'store' ? '설정 탭에서 지정한 값 사용 중'
    : eng.source === 'env' ? '.env 파일 값 사용 중 (아래에 저장하면 이 값을 덮어씁니다)'
    : '엔진 미설정 — 일기·채팅이 비활성 상태예요',
  );

  // --- 캐릭터 이미지 모델 (사내 LLM은 그림을 못 그려 텍스트 엔진과 분리) ---
  let img = $state<ImageSettings>({ url:'', key:'', model:'', headers:'', api:'chat', source:'none' });
  let imgStatus = $state<Status>(IDLE);
  const IMAGE_APIS: { value: ImageApi; label: string }[] = [
    { value: 'chat',   label: 'chat/completions (참조 이미지 첨부)' },
    { value: 'images', label: 'images/generations (OpenAI 규격)' },
  ];
  async function loadImage(){
    try { img = await imageSettingsGet(); } catch(e){ imgStatus = err(`이미지 설정을 불러오지 못했어요: ${e}`); }
  }
  loadImage();
  async function saveImage(){
    imgStatus = busy('저장 중…');
    try { await imageSettingsSet(img.url, img.key, img.model, img.headers, img.api); await loadImage(); imgStatus = ok('저장했어요.'); }
    catch(e){ imgStatus = err(e); }
  }
  async function testImage(){
    imgStatus = busy('연결 확인 중…');
    // image_test는 실패 시 이미 완성된 한국어 메시지를 reject하므로 접두어 없이 그대로 쓴다.
    try { imgStatus = ok(await imageTest(img.url, img.key, img.model, img.headers, img.api)); }
    catch(e){ imgStatus = err(e); }
  }
  const imgSourceLabel = $derived(
    img.source === 'store' ? '설정 탭에서 지정한 값 사용 중'
    : img.source === 'env' ? '.env 값 사용 중'
    : '미설정 — 캐릭터가 기본 그림으로 표시됩니다',
  );

  // --- A-Lens (관전 웹) 주소 — 홈 화면의 'A-Lens에서 보기' 링크가 쓴다 ---
  // 링크는 <주소>/#life/<기록할 공간>으로 조립되므로, 공간은 아래 지식 허브 설정을 따른다.
  // 주소가 없으면 링크를 숨기는 조립 규칙은 lib/lens.ts와 공유한다.
  let lensUrl = $state('');
  let lensStatus = $state<Status>(IDLE);
  async function loadLens(){
    try {
      const all = await getSettings();
      lensUrl = (all.a_lens_url ?? '').trim();
    } catch(e){ lensStatus = err(`A-Lens 주소를 불러오지 못했어요: ${e}`); }
  }
  loadLens();
  async function saveLens(){
    lensStatus = busy('저장 중…');
    // 빈 값과 과거 설정의 'off' 모두 링크를 숨긴다(lens.ts 규칙).
    try {
      await setSetting('a_lens_url', lensUrl.trim());
      lensStatus = ok(
        !lensUrl.trim() || lensUrl.trim().toLowerCase() === LENS_OFF
          ? '홈 화면 링크를 숨깁니다.'
          : '저장했어요. 홈 화면 링크에 반영됩니다.',
      );
    } catch(e){ lensStatus = err(e); }
  }

  // --- 팀 지식 허브 (a-hub work) — Life Server와 다른 서버다 ---
  // 입력 칸에만 보여줄 팀 배포 예시. 저장하기 전에는 연결값으로 쓰지 않는다.
  const KHUB_EXAMPLES = {
    url: 'https://spacea.msalt.net',
    space_id: 'sw-innov',
  } as const;
  let khub = $state<KnowledgeHubSettings>({ url:'', api_key:'', space_id:'', user:'', source:'none', share_off:false });
  let khubStatus = $state<Status>(IDLE);
  async function loadKhub(){
    try {
      khub = await knowledgeHubSettingsGet();
      if (!khub.space_id.trim()) khub.space_id = KHUB_EXAMPLES.space_id;
    }
    catch(e){ khubStatus = err(`지식 허브 설정을 불러오지 못했어요: ${e}`); }
  }
  loadKhub();
  async function saveKhub(){
    khubStatus = busy('저장 중…');
    const url = khub.url.trim();
    const spaceId = khub.space_id.trim() || KHUB_EXAMPLES.space_id;
    try {
      await knowledgeHubSettingsSet(url, khub.api_key, spaceId, khub.user);
      // URL을 저장할 때만 켠다. 빈 값으로 저장값을 지울 때는 기존 공유 off 상태를 보존한다.
      if (url) await knowledgeHubShareSet(true);
      await loadKhub();
      khubStatus = ok(
        url
          ? '설정을 저장했어요. 관문 서버는 유효한 API 키가 있어야 연결됩니다.'
          : khub.share_off
            ? '저장된 URL을 지웠어요. 공유 꺼짐 상태는 유지됩니다.'
            : khub.source === 'env'
              ? '저장된 URL을 지웠어요. 이제 .env 설정을 사용합니다.'
              : '저장된 URL을 지웠어요. 외부에 연결하지 않습니다.',
      );
    } catch(e){ khubStatus = err(e); }
  }
  async function toggleKhubShare(){
    const turningOn = khub.share_off;
    khubStatus = busy(turningOn ? '켜는 중…' : '끄는 중…');
    try {
      await knowledgeHubShareSet(turningOn);
      await loadKhub();
      khubStatus = ok(turningOn
        ? '팀 지식 공유를 다시 켰어요.'
        : '팀 지식 공유를 껐어요. 발행·인용을 더 이상 하지 않습니다.');
    } catch(e){ khubStatus = err(e); }
  }
  const khubSourceLabel = $derived(
    khub.share_off ? '공유 꺼짐 — 팀에 아무것도 보내지 않습니다'
    : khub.source === 'store' ? `설정됨 — ${khub.space_id || '공간 미지정'} 공간을 사용합니다`
    : khub.source === 'env' ? '.env 설정 사용 중 (설치본에서는 로드되지 않으니 아래에 저장해 두세요)'
    : '미연결 — URL을 저장하기 전에는 외부에 요청하지 않습니다',
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
  <h2>A-Lens (관전 웹)</h2>
  <p class="hint">
    우리 팀 방을 브라우저에서 구경하는 화면입니다. 홈 화면 왼쪽 아래 'A-Lens에서 보기'가
    <em>{`<주소>/#life/<기록할 공간>`}</em> 로 열립니다 — 공간은 아래 지식 허브 설정을 따릅니다.
  </p>
  <div class="fields">
    <label class="field"><span>A-Lens 주소 <em>(비우면 숨김)</em></span>
      <input type="text" bind:value={lensUrl} placeholder={LENS_URL_PLACEHOLDER} spellcheck="false"/></label>
  </div>
  <div class="actions">
    <button class="primary" onclick={saveLens} disabled={lensStatus.kind==='busy'}>저장</button>
  </div>
  <StatusLine status={lensStatus}/>
</section>

<section>
  <h2>팀 지식 허브</h2>
  <p class="hint">
    내 에이전트가 찾아낸 해결책을 팀에 발행하고, 남이 이미 올린 지식이 있으면 대신 인용합니다 (Space A).
    위 Life Server와는 <em>다른 서버</em>입니다. 프롬프트 원문·파일 경로·세션 ID는 보내지 않습니다.
  </p>
  <p class="source" data-kind={khub.share_off || khub.source === 'none' ? 'none' : khub.source}>{khubSourceLabel}</p>
  <div class="fields">
    <label class="field"><span>허브 URL</span>
      <input type="text" bind:value={khub.url} placeholder={KHUB_EXAMPLES.url} spellcheck="false"/></label>
    <label class="field"><span>API 키 <em>(관문 켜진 서버만)</em></span>
      <input type="password" bind:value={khub.api_key} placeholder="비워두면 인증 없이" spellcheck="false"/></label>
    <label class="field"><span>기록할 공간</span>
      <input type="text" bind:value={khub.space_id} placeholder={KHUB_EXAMPLES.space_id} spellcheck="false"/></label>
    <label class="field"><span>내 이름 <em>(허브 계정)</em></span>
      <input type="text" bind:value={khub.user} placeholder="예: palendy" spellcheck="false"/></label>
  </div>
  <div class="actions">
    <button class="primary" onclick={saveKhub} disabled={khubStatus.kind==='busy'}>저장</button>
    {#if khub.source !== 'none' || khub.share_off}
      <button onclick={toggleKhubShare} disabled={khubStatus.kind==='busy'}>
        {khub.share_off ? '공유 켜기' : '공유 끄기'}
      </button>
    {/if}
  </div>
  <StatusLine status={khubStatus}/>
  <p class="hint2">
    팀 배포 예시: {KHUB_EXAMPLES.url} · {KHUB_EXAMPLES.space_id}. 관문이 켜진 서버는
    별도로 전달받은 API 키가 필요하며, 키는 앱 설치본에 포함되지 않습니다.
  </p>
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
    <label class="field wide"><span>추가 헤더 <em>(선택 — 한 줄에 하나씩 <code>이름: 값</code>)</em></span>
      <textarea bind:value={eng.headers} rows="3" placeholder={HEADER_PLACEHOLDER} spellcheck="false"></textarea>
      {#if nonAsciiNames(eng.headers).length}
        <span class="warn">{nonAsciiNames(eng.headers).join(', ')} — 값에 한글 등 비ASCII가 있어 <b>UTF-8 퍼센트 인코딩</b>해서 보냅니다 (HTTP 헤더는 ASCII만 실을 수 있습니다). 서버가 디코드하지 않는다면 값을 영문으로 바꾸세요.</span>
      {/if}</label>
  </div>
  <div class="actions">
    <button class="primary" onclick={saveEngine} disabled={engStatus.kind==='busy'}>저장</button>
    <button onclick={testEngine} disabled={engStatus.kind==='busy'}>연결 테스트</button>
  </div>
  <StatusLine status={engStatus}/>
  <p class="hint2">
    사내 게이트웨이가 신원 헤더를 요구하면 위에 적으세요 — 모든 호출(일기·한마디·잡담·채팅)에 함께 나갑니다.
    URL을 비우고 저장하면 .env(AGENT_MENTOR_ENGINE_*) 값으로 되돌아갑니다.
  </p>
</section>

<section>
  <h2>캐릭터 이미지</h2>
  <p class="hint">마스코트 캐릭터를 그릴 이미지 생성 모델입니다. 사내 LLM은 그림을 못 그리므로 위 텍스트 엔진과 따로 지정합니다. 사람마다 한 번 생성해 캐시하므로 이후에는 호출하지 않습니다. 캐릭터 생성·재생성과 '오늘의 대문사진'은 설정 &gt; 봇에서 합니다.</p>
  <p class="source" data-kind={img.source}>{imgSourceLabel}</p>
  <div class="fields">
    <label class="field"><span>엔드포인트 URL</span>
      <input type="text" bind:value={img.url} placeholder="https://openrouter.ai/api/v1" spellcheck="false"/></label>
    <label class="field"><span>API 키</span>
      <input type="password" bind:value={img.key} placeholder="sk-or-..." spellcheck="false"/></label>
    <label class="field"><span>이미지 모델</span>
      <input type="text" bind:value={img.model} placeholder="google/gemini-2.5-flash-image" spellcheck="false"/></label>
    <label class="field"><span>API 방식</span>
      <select bind:value={img.api}>
        {#each IMAGE_APIS as a (a.value)}<option value={a.value}>{a.label}</option>{/each}
      </select></label>
    <label class="field wide"><span>추가 헤더 <em>(선택 — 한 줄에 하나씩 <code>이름: 값</code>)</em></span>
      <textarea bind:value={img.headers} rows="3" placeholder={HEADER_PLACEHOLDER} spellcheck="false"></textarea>
      {#if nonAsciiNames(img.headers).length}
        <span class="warn">{nonAsciiNames(img.headers).join(', ')} — 값에 한글 등 비ASCII가 있어 <b>UTF-8 퍼센트 인코딩</b>해서 보냅니다.</span>
      {/if}</label>
  </div>
  <div class="actions">
    <button class="primary" onclick={saveImage} disabled={imgStatus.kind==='busy'}>저장</button>
    <button onclick={testImage} disabled={imgStatus.kind==='busy'}>연결 테스트</button>
  </div>
  <StatusLine status={imgStatus}/>
  <p class="hint2">
    <b>API 방식</b> — OpenRouter·LiteLLM처럼 <code>chat/completions</code>로 그림을 주는 게이트웨이는 첫 번째를
    고르세요. 화풍 견본 이미지를 함께 보내 캐릭터가 일관됩니다. 사내 게이트웨이처럼
    <code>images/generations</code>만 여는 곳은 두 번째를 고르세요 — 이 규격은 참조 이미지를 받지 못해
    화풍을 글로만 지시합니다.
  </p>
</section>

<style>
  .source{margin:8px 0 0;font-size:12px;padding:7px 11px;border-radius:8px;background:var(--lav-surface);color:var(--lav-ink)}
  .source[data-kind=none]{background:var(--surface-inset);color:var(--danger)}
  .source[data-kind=env]{background:var(--cream);color:var(--cream-ink)}
  .fields{display:grid;grid-template-columns:repeat(2,1fr);gap:10px;margin-top:12px}
  .field{display:flex;flex-direction:column;gap:4px}
  .field>span{font-size:12px;color:var(--text-soft)}
  .field em{font-style:normal;opacity:.7}
  .field input,.field select,.field textarea{border:1px solid var(--line);border-radius:8px;padding:8px 10px;background:var(--surface-inset);color:var(--text);font:inherit}
  /* 헤더는 여러 줄이라 2열 그리드 전체를 쓴다 */
  .field.wide{grid-column:1 / -1}
  .field textarea{resize:vertical;min-height:64px;font-family:ui-monospace,SFMono-Regular,Menlo,monospace;font-size:12px;white-space:pre}
  .field code{font-family:ui-monospace,SFMono-Regular,Menlo,monospace}
  .warn{font-size:11px;color:var(--cream-ink);background:var(--cream);border-radius:6px;padding:5px 8px}
  .actions{display:flex;gap:8px;margin-top:12px}
  .actions button{border:0;border-radius:99px;padding:7px 14px;background:var(--lav-surface);color:var(--lav-ink);cursor:pointer;font:inherit}
  .actions button.primary{background:var(--accent);color:var(--accent-ink);font-weight:700}
  .actions button:disabled{opacity:.55;cursor:default}
  .hint2{color:var(--text-soft);font-size:11px;margin:8px 0 0}
</style>
