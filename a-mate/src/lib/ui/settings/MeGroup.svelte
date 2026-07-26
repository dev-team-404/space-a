<script lang="ts">
  import {
    memoryAdd, memoryDelete, memoryList, memoryUpdate,
    profileGet, profileSet, mascotPreview, mascotCommit, getSprite,
    type Memory, type Profile,
  } from '../../api';
  import { IDLE, busy, err, ok, type Status } from './status';
  import StatusLine from './StatusLine.svelte';

  const MBTI_OPTIONS = ['', 'ISTJ','ISFJ','INFJ','INTJ','ISTP','ISFP','INFP','INTP','ESTP','ESFP','ENFP','ENTP','ESTJ','ESFJ','ENFJ','ENTJ'];
  let prof = $state<Profile>({ name: '', org: '', uuid: '', mbti: '', owner_title: '주인' });
  let profMbti = $state('');
  let profStatus = $state<Status>(IDLE);
  async function loadProfile(){ try { prof = await profileGet(); profMbti = prof.mbti; } catch(e){ profStatus = err(e); } }
  loadProfile();
  async function saveProfile(){
    profStatus = busy('저장 중…');
    try {
      prof = await profileSet(prof.name, prof.org, profMbti, prof.owner_title);
      profMbti = prof.mbti;
      profStatus = ok('저장했어요.');
    } catch(e){ profStatus = err(e); }
  }

  let sprite = $state<string | null>(null);
  let hasCandidate = $state(false);
  let genStatus = $state<Status>(IDLE);
  async function loadSprite(){ sprite = await getSprite(); }
  loadSprite();
  async function regenMascot(){
    genStatus = busy('마스코트 그리는 중… 수십 초 걸릴 수 있어요');
    try { sprite = await mascotPreview(); hasCandidate = true; genStatus = ok('미리보기 완성 — 마음에 들면 저장하세요.'); }
    catch(e){ genStatus = err(`생성 실패: ${e}`); }
  }
  async function saveMascot(){
    genStatus = busy('저장 중…');
    try { await mascotCommit(); hasCandidate = false; genStatus = ok('마스코트를 저장했어요!'); }
    catch(e){ genStatus = err(e); }
  }

  let memories = $state<Memory[]>([]);
  let memInput = $state(''), memEditId = $state<number | null>(null), memEditText = $state('');
  let memStatus = $state<Status>(IDLE);
  async function loadMemories(){ try { memories = await memoryList(); } catch(e){ memStatus = err(e); } }
  loadMemories();
  async function addMemory(){
    const t = memInput.trim(); if(!t) return;
    try { await memoryAdd(t); memInput=''; memStatus=IDLE; await loadMemories(); } catch(e){ memStatus = err(e); }
  }
  function startEdit(m: Memory){ memEditId = m.id; memEditText = m.text; }
  async function saveEdit(){
    if(memEditId==null) return;
    const t = memEditText.trim(); if(!t) return;
    try { await memoryUpdate(memEditId, t); memEditId=null; memStatus=IDLE; await loadMemories(); } catch(e){ memStatus = err(e); }
  }
  async function removeMemory(id: number){
    try { await memoryDelete(id); memStatus=IDLE; await loadMemories(); } catch(e){ memStatus = err(e); }
  }
</script>

<section>
  <h2>마스코트 정보</h2>
  <p class="hint">봇(마스코트)의 이름·성향입니다. 아이디는 자동 부여되며 바뀌지 않아요.</p>
  <div class="fields">
    <label class="field"><span>마스코트 이름</span><input type="text" bind:value={prof.name} placeholder="예: 둘쇠" spellcheck="false"/></label>
    <label class="field"><span>조직</span><input type="text" bind:value={prof.org} placeholder="S/W 혁신팀" spellcheck="false"/></label>
    <label class="field"><span>호칭 <em>(주인을 부르는 말)</em></span><input type="text" bind:value={prof.owner_title} placeholder="주인" spellcheck="false"/></label>
    <label class="field"><span>아이디</span><input type="text" value={prof.uuid} readonly title="자동 부여된 고유 ID"/></label>
    <label class="field"><span>MBTI <em>(선택)</em></span>
      <select bind:value={profMbti}>{#each MBTI_OPTIONS as m}<option value={m}>{m === '' ? '미설정' : m}</option>{/each}</select></label>
  </div>
  <p class="hint2">MBTI는 마스코트 외관 성향과 말투에 반영돼요. 외관은 아래 '마스코트 생성'에서 재생성·저장해야 실제로 바뀝니다.</p>
  <div class="actions">
    <button class="primary" onclick={saveProfile} disabled={profStatus.kind==='busy'}>저장</button>
  </div>
  <StatusLine status={profStatus}/>
</section>

<section>
  <h2>마스코트 생성</h2>
  <p class="hint">MBTI·성향에 맞춰 마스코트를 그립니다. '재생성'으로 미리보고 '저장'을 눌러야 실제로 반영돼요. (연결 탭의 캐릭터 이미지 모델 설정 필요)</p>
  <div class="preview">
    {#if sprite}
      <img src={`data:image/png;base64,${sprite}`} alt="마스코트 미리보기"/>
    {:else}
      <span class="noimg">아직 생성된 마스코트가 없어요.</span>
    {/if}
  </div>
  <div class="actions">
    <button onclick={regenMascot} disabled={genStatus.kind==='busy'}>재생성</button>
    <button class="primary" onclick={saveMascot} disabled={!hasCandidate || genStatus.kind==='busy'}>저장</button>
  </div>
  <StatusLine status={genStatus}/>
</section>

<section>
  <h2>주인 메모리</h2>
  <p class="hint">마스코트가 기억할 나에 대한 사실이에요. 채팅에서 "기억해둬"라고 하거나 여기서 직접 추가할 수 있어요. (엔진으로 전송됩니다)</p>
  <div class="memadd">
    <input type="text" bind:value={memInput} placeholder="예: 나는 비건이야" onkeydown={(e)=>{if(e.key==='Enter')addMemory();}}/>
    <button class="primary" onclick={addMemory}>추가</button>
  </div>
  <ul class="memlist">
    {#each memories as m (m.id)}
      <li>
        {#if memEditId===m.id}
          <input type="text" bind:value={memEditText} onkeydown={(e)=>{if(e.key==='Enter')saveEdit();}}/>
          <button onclick={saveEdit}>저장</button>
          <button onclick={()=>memEditId=null}>취소</button>
        {:else}
          <span class="memtext">{m.text}</span>
          <span class="memdate">{m.created_at}</span>
          <button onclick={()=>startEdit(m)}>편집</button>
          <button onclick={()=>removeMemory(m.id)}>삭제</button>
        {/if}
      </li>
    {:else}
      <li class="memempty">아직 기억한 게 없어요.</li>
    {/each}
  </ul>
  <StatusLine status={memStatus}/>
</section>

<style>
  .fields{display:grid;grid-template-columns:repeat(2,1fr);gap:10px;margin-top:12px}
  .field{display:flex;flex-direction:column;gap:4px}
  .field>span{font-size:12px;color:var(--text-soft)}
  .field em{font-style:normal;opacity:.7}
  .field input,.field select{border:1px solid var(--line);border-radius:8px;padding:8px 10px;background:var(--surface-inset);color:var(--text);font:inherit}
  .field input[readonly]{opacity:.7;cursor:default}
  .actions{display:flex;gap:8px;margin-top:12px}
  .actions button{border:0;border-radius:99px;padding:7px 14px;background:var(--lav-surface);color:var(--lav-ink);cursor:pointer;font:inherit}
  .actions button.primary{background:var(--accent);color:var(--accent-ink);font-weight:700}
  .actions button:disabled{opacity:.55;cursor:default}
  .hint2{color:var(--text-soft);font-size:11px;margin:8px 0 0}
  .preview{margin-top:12px;display:flex;align-items:center;justify-content:center;min-height:140px;background:var(--surface-inset);border:1px solid var(--line);border-radius:8px}
  .preview img{max-height:180px;image-rendering:pixelated}
  .preview .noimg{color:var(--text-soft);font-size:12px}
  .memadd{display:flex;gap:8px;margin-top:12px}
  .memadd input{flex:1;border:1px solid var(--line);border-radius:8px;padding:8px 10px;background:var(--surface-inset);color:var(--text);font:inherit}
  .memadd button{border:0;border-radius:99px;padding:7px 14px;background:var(--accent);color:var(--accent-ink);font-weight:700;cursor:pointer;font-size:inherit}
  .memlist{list-style:none;padding:0;margin:12px 0 0;display:flex;flex-direction:column;gap:6px}
  .memlist li{display:flex;align-items:center;gap:8px;padding:8px 10px;background:var(--cream);color:var(--cream-ink);border-radius:8px}
  .memlist .memtext{flex:1}
  .memlist .memdate{font-size:11px;color:var(--text-soft)}
  .memlist input{flex:1;border:1px solid var(--line);border-radius:6px;padding:6px 8px;background:var(--surface-inset);color:var(--text);font:inherit}
  .memlist button{border:0;border-radius:99px;padding:5px 10px;background:var(--lav-surface);color:var(--lav-ink);cursor:pointer;font-size:12px}
  .memempty{color:var(--text-soft);justify-content:center}
</style>
