<script lang="ts">
  import {
    getDiary, lifeContentAccess, lifePeople, lifeSetContentVisibility,
    lifeSetDiaryVisibility, lifeSetFriend, lifeView, listDiaryDates,
    type ContentVisibility, type LifePerson,
  } from '../../api';
  import { IDLE, err, type Status } from './status';
  import StatusLine from './StatusLine.svelte';

  // 공개 범위 타입은 api.ts의 ContentVisibility가 원본 — 로컬에 다시 정의하지 않는다.
  const VISIBILITIES: [ContentVisibility, string][] = [['private','비공개'],['friends','일촌공개'],['public','전체공개']];

  let people = $state<LifePerson[]>([]);
  let status = $state<Status>(IDLE);
  let visibility = $state<ContentVisibility>((localStorage.getItem('life-diary-visibility') as ContentVisibility) || 'private');
  let sharing = $state(false);

  async function load(){
    try {
      people = (await lifePeople()).people;
      const view = await lifeView();
      visibility = (await lifeContentAccess(view.me.my_life_id)).features.diary.visibility;
    } catch(e){ status = err(e); }
  }
  load();

  async function toggle(person: LifePerson){ await lifeSetFriend(person.agent_id, !person.is_friend); await load(); }

  async function setVisibility(next: ContentVisibility){
    sharing = true; status = IDLE;
    try {
      await lifeSetContentVisibility('diary', next);
      if (next !== 'private') {
        const dates = await listDiaryDates();
        await Promise.all(dates.map(async (date) => lifeSetDiaryVisibility(date, (await getDiary(date)) || '', next)));
      }
      visibility = next;
      localStorage.setItem('life-diary-visibility', next);
    } catch(e){ status = err(e); }
    finally { sharing = false; }
  }
</script>

<section>
  <h2>일촌 관리</h2>
  <p class="hint">내가 일촌공개로 설정한 콘텐츠를 볼 수 있는 사람을 관리합니다.</p>
  <div class="people">
    {#each people as person (person.agent_id)}
      <label><span>{person.name}</span><input type="checkbox" checked={person.is_friend} onchange={()=>toggle(person)}/></label>
    {:else}
      <p class="hint">등록된 다른 사용자가 없어요.</p>
    {/each}
  </div>
</section>

<section>
  <h2>다이어리 공개 범위</h2>
  <p class="hint">비공개가 기본이며, 공개를 선택한 경우에만 다이어리가 Life 서버로 전송됩니다.</p>
  <nav class="seg">
    {#each VISIBILITIES as option (option[0])}
      <button class:active={visibility===option[0]} disabled={sharing} onclick={()=>setVisibility(option[0])}>{option[1]}</button>
    {/each}
  </nav>
  <StatusLine {status}/>
</section>

<style>
  .people{display:grid;grid-template-columns:repeat(2,1fr);gap:8px;margin-top:14px}
  .people label{display:flex;justify-content:space-between;padding:10px 12px;background:var(--cream);color:var(--cream-ink);border-radius:9px}
  .seg{display:inline-flex;border:1px solid var(--line);border-radius:9px;overflow:hidden;margin-top:10px}
  .seg button{border:0;border-right:1px solid var(--line);border-radius:0;background:transparent;color:var(--text);padding:7px 16px;cursor:pointer;font:inherit}
  .seg button:last-child{border-right:0}
  .seg button.active{background:var(--accent);color:var(--accent-ink);font-weight:700}
  .seg button:disabled{opacity:.55;cursor:default}
</style>
