<script lang="ts">
  import { getDiary, lifeContentAccess, lifePeople, lifeSetContentVisibility, lifeSetDiaryVisibility, lifeSetFriend, lifeView, listDiaryDates, memoryAdd, memoryDelete, memoryList, memoryUpdate, profileGet, profileSet, regenerateSprite, type LifePerson, type Memory, type Profile } from '../api';
  import { getTheme, setTheme, type ThemeMode, type ThemeSkin } from '../theme';

  // --- 개인정보 (a-hub 연결·마스코트 시드에 사용) ---
  const MBTI_OPTIONS = ['', 'ISTJ','ISFJ','INFJ','INTJ','ISTP','ISFP','INFP','INTP','ESTP','ESFP','ENFP','ENTP','ESTJ','ESFJ','ENFJ','ENTJ'];
  let prof = $state<Profile>({ name: '', org: '', uuid: '', mbti: '' });
  let profMbti = $state('');
  let profStatus = $state<{ kind: 'idle'|'ok'|'err'|'busy'; text: string }>({ kind: 'idle', text: '' });
  async function loadProfile(){ try { prof = await profileGet(); profMbti = prof.mbti; } catch(e){ profStatus = { kind:'err', text:`${e}` }; } }
  loadProfile();
  async function saveProfile(){
    profStatus = { kind:'busy', text:'저장 중…' };
    const mbtiChanged = profMbti !== prof.mbti;
    try {
      prof = await profileSet(prof.name, prof.org, profMbti);
      profMbti = prof.mbti;
      if (mbtiChanged) {
        profStatus = { kind:'busy', text:'MBTI가 바뀌어 캐릭터를 다시 그리는 중… (수십 초)' };
        try { await regenerateSprite(); profStatus = { kind:'ok', text:'저장 + 새 캐릭터 반영 완료' }; }
        catch(e){ profStatus = { kind:'ok', text:`저장됨. 캐릭터 재생성은 실패(이미지 모델 설정 확인): ${e}` }; }
      } else {
        profStatus = { kind:'ok', text:'저장했어요.' };
      }
    } catch(e){ profStatus = { kind:'err', text:`${e}` }; }
  }
  let people=$state<LifePerson[]>([]),error=$state('');
  let visibility=$state<'private'|'friends'|'public'>((localStorage.getItem('life-diary-visibility') as any)||'private'),sharing=$state(false);
  async function load(){try{people=(await lifePeople()).people;const view=await lifeView();visibility=(await lifeContentAccess(view.me.my_life_id)).features.diary.visibility}catch(e){error=`${e}`}}load();
  async function toggle(person:LifePerson){await lifeSetFriend(person.agent_id,!person.is_friend);await load()}
  async function setVisibility(next:typeof visibility){sharing=true;error='';try{await lifeSetContentVisibility('diary',next);if(next!=='private'){const dates=await listDiaryDates();await Promise.all(dates.map(async date=>lifeSetDiaryVisibility(date,await getDiary(date)||'',next)))}visibility=next;localStorage.setItem('life-diary-visibility',next)}catch(e){error=`${e}`}finally{sharing=false}}
  let themeMode=$state<ThemeMode>(getTheme().mode),themeSkin=$state<ThemeSkin>(getTheme().skin);
  const MODES:[ThemeMode,string][]=[['light','라이트'],['dark','다크'],['system','시스템']];
  const SKINS:{id:ThemeSkin;label:string;dot:string}[]=[{id:'sky',label:'하늘',dot:'#84c9ef'},{id:'mint',label:'민트',dot:'#5cd0b0'},{id:'peach',label:'살구',dot:'#f4b183'},{id:'lavender',label:'라벤더',dot:'#b9a6f0'}];
  function pickMode(m:ThemeMode){themeMode=m;setTheme({mode:m,skin:themeSkin})}
  function pickSkin(s:ThemeSkin){themeSkin=s;setTheme({mode:themeMode,skin:s})}

  // --- 주인 메모리 ---
  let memories = $state<Memory[]>([]);
  let memInput = $state('');
  let memEditId = $state<number | null>(null);
  let memEditText = $state('');
  let memErr = $state('');
  async function loadMemories(){ try { memories = await memoryList(); } catch(e){ memErr = `${e}`; } }
  loadMemories();
  async function addMemory(){
    const t = memInput.trim(); if(!t) return;
    try { await memoryAdd(t); memInput=''; memErr=''; await loadMemories(); } catch(e){ memErr = `${e}`; }
  }
  function startEdit(m: Memory){ memEditId = m.id; memEditText = m.text; }
  async function saveEdit(){
    if(memEditId==null) return;
    const t = memEditText.trim(); if(!t) return;
    try { await memoryUpdate(memEditId, t); memEditId=null; memErr=''; await loadMemories(); } catch(e){ memErr = `${e}`; }
  }
  async function removeMemory(id: number){
    try { await memoryDelete(id); memErr=''; await loadMemories(); } catch(e){ memErr = `${e}`; }
  }
</script>
<section><h2>개인정보</h2><p>a-hub 연결과 마스코트 캐릭터에 쓰입니다. 아이디는 자동 부여되며 바뀌지 않아요.</p>
<div class="profile">
  <label class="field"><span>이름</span><input type="text" bind:value={prof.name} placeholder="예: 준녕" spellcheck="false"/></label>
  <label class="field"><span>조직</span><input type="text" bind:value={prof.org} placeholder="S/W 혁신팀" spellcheck="false"/></label>
  <label class="field"><span>아이디</span><input type="text" value={prof.uuid} readonly title="자동 부여된 고유 ID"/></label>
  <label class="field"><span>MBTI <em>(선택)</em></span>
    <select bind:value={profMbti}>{#each MBTI_OPTIONS as m}<option value={m}>{m === '' ? '미설정' : m}</option>{/each}</select>
  </label>
</div>
<div class="prow"><button class="save" onclick={saveProfile} disabled={profStatus.kind==='busy'}>저장</button>
{#if profStatus.text}<span class="pstatus" data-kind={profStatus.kind}>{profStatus.text}</span>{/if}</div>
<p class="hint2">MBTI를 바꾸고 저장하면 그 성향에 맞춰 캐릭터를 다시 그립니다. (이미지 모델 설정 필요)</p>
<hr/><h2>테마</h2><p>밝기</p><nav class="seg">{#each MODES as opt}<button class:active={themeMode===opt[0]} onclick={()=>pickMode(opt[0])}>{opt[1]}</button>{/each}</nav><p>색상 세트</p><div class="skins">{#each SKINS as s (s.id)}<button class="swatch" class:active={themeSkin===s.id} onclick={()=>pickSkin(s.id)} title={s.label}><span class="dot" style="background:{s.dot}"></span>{s.label}</button>{/each}</div><hr/><h2>일촌 관리</h2><p>내가 일촌공개로 설정한 콘텐츠를 볼 수 있는 사람을 관리합니다.</p><div>{#each people as person (person.agent_id)}<label><span>{person.name}</span><input type="checkbox" checked={person.is_friend} onchange={()=>toggle(person)}/></label>{:else}<p>등록된 다른 사용자가 없어요.</p>{/each}</div><hr/><h2>다이어리 공개 범위</h2><p>비공개가 기본이며, 공개를 선택한 경우에만 다이어리가 Life 서버로 전송됩니다.</p><nav>{#each [['private','비공개'],['friends','일촌공개'],['public','전체공개']] as option}<button class:active={visibility===option[0]} disabled={sharing} onclick={()=>setVisibility(option[0] as typeof visibility)}>{option[1]}</button>{/each}</nav>{#if error}<p class="error">{error}</p>{/if}
<hr/><h2>주인 메모리</h2><p>마스코트가 기억할 나에 대한 사실이에요. 채팅에서 "기억해둬"라고 하거나 여기서 직접 추가할 수 있어요. (엔진으로 전송됩니다)</p>
<div class="memadd">
  <input type="text" bind:value={memInput} placeholder="예: 나는 비건이야" onkeydown={(e)=>{if(e.key==='Enter')addMemory();}} />
  <button class="save" onclick={addMemory}>추가</button>
</div>
<ul class="memlist">
  {#each memories as m (m.id)}
    <li>
      {#if memEditId===m.id}
        <input type="text" bind:value={memEditText} onkeydown={(e)=>{if(e.key==='Enter')saveEdit();}} />
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
{#if memErr}<p class="error">{memErr}</p>{/if}</section>
<style>section{padding:18px}h2{margin:0 0 5px;font-size:16px}section>p{color:var(--text-soft);font-size:12px}section>div{display:grid;grid-template-columns:repeat(2,1fr);gap:8px;margin-top:14px}label{display:flex;justify-content:space-between;padding:10px 12px;background:var(--cream);color:var(--cream-ink);border-radius:9px}nav{display:flex;gap:7px;margin-top:12px}button{border:0;border-radius:99px;padding:7px 12px;background:var(--lav-surface);color:var(--lav-ink);cursor:pointer}button.active{background:var(--accent);color:var(--accent-ink)}hr{border:0;border-top:1px solid var(--line);margin:20px 0}.error{color:var(--danger)}.seg{display:inline-flex;border:1px solid var(--line);border-radius:9px;overflow:hidden;margin-top:6px}.seg button{border:0;border-right:1px solid var(--line);border-radius:0;background:transparent;color:var(--text);padding:7px 16px}.seg button:last-child{border-right:0}.seg button.active{background:var(--accent);color:var(--accent-ink);font-weight:700}.skins{display:flex;gap:10px;flex-wrap:wrap;margin-top:6px}.swatch{display:flex;align-items:center;gap:6px;border:1px solid var(--line);background:var(--surface-inset);color:var(--text);border-radius:99px;padding:5px 12px 5px 6px}.swatch.active{border-color:var(--accent-strong);font-weight:700}.swatch .dot{width:16px;height:16px;border-radius:50%;border:1px solid var(--line)}
.profile{display:grid;grid-template-columns:repeat(2,1fr);gap:10px;margin-top:12px}
.field{display:flex;flex-direction:column;gap:4px;background:transparent;padding:0}
.field>span{font-size:12px;color:var(--text-soft)}.field em{font-style:normal;opacity:.7}
.field input,.field select{border:1px solid var(--line);border-radius:8px;padding:8px 10px;background:var(--surface-inset);color:var(--text);font:inherit}
.field input[readonly]{opacity:.7;cursor:default}
.prow{display:flex;align-items:center;gap:10px;margin-top:12px}
.save{background:var(--accent);color:var(--accent-ink);font-weight:700}
.pstatus{font-size:12px;color:var(--text-soft)}.pstatus[data-kind=err]{color:var(--danger)}.pstatus[data-kind=ok]{color:var(--accent-strong)}
.hint2{color:var(--text-soft);font-size:11px;margin-top:8px}
.memadd{display:flex;gap:8px;margin-top:12px}
.memadd input{flex:1;border:1px solid var(--line);border-radius:8px;padding:8px 10px;background:var(--surface-inset);color:var(--text);font:inherit}
.memlist{list-style:none;padding:0;margin:12px 0 0;display:flex;flex-direction:column;gap:6px}
.memlist li{display:flex;align-items:center;gap:8px;padding:8px 10px;background:var(--cream);color:var(--cream-ink);border-radius:8px}
.memlist .memtext{flex:1}
.memlist .memdate{font-size:11px;color:var(--text-soft)}
.memlist input{flex:1;border:1px solid var(--line);border-radius:6px;padding:6px 8px;background:var(--surface-inset);color:var(--text);font:inherit}
.memlist button{border:0;border-radius:99px;padding:5px 10px;background:var(--lav-surface);color:var(--lav-ink);cursor:pointer;font-size:12px}
.memempty{color:var(--text-soft);justify-content:center}</style>
