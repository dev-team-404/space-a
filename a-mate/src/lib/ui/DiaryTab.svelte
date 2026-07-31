<script lang="ts">
  import { marked } from 'marked';
  import DOMPurify from 'dompurify';
  import { getDiary, lifeDiaries, listDiaryDates, onDiaryReady, type SharedDiary } from '../api';
  import { monthGrid, shiftMonth } from './calendar';
  import DayActivity from './diary/DayActivity.svelte';

  let { focusDate = null, visiting = false, lifeId = '' }: { focusDate?: string|null; visiting?: boolean; lifeId?: string } = $props();
  const now=new Date();let year=$state(now.getFullYear()),month=$state(now.getMonth()+1),dates=$state<Set<string>>(new Set()),selected=$state<string|null>(null),html=$state<string|null>(null);
  let remote=$state<Map<string,SharedDiary>>(new Map());
  const grid=$derived(monthGrid(year,month));
  let loadSequence=0;
  async function loadDates(){
    const sequence=++loadSequence,targetVisiting=visiting,targetLifeId=lifeId;
    if(targetVisiting){
      const rows=(await lifeDiaries(targetLifeId)).diaries;
      if(sequence!==loadSequence)return;
      remote=new Map(rows.map(r=>[r.date,r]));dates=new Set(remote.keys())
    }
    else{
      const rows=await listDiaryDates().catch(()=>[]);
      if(sequence!==loadSequence)return;
      dates=new Set(rows)
    }
  }
  $effect(()=>{visiting;lifeId;loadDates().catch(()=>{})});
  $effect(()=>{if(visiting)return;const p=onDiaryReady(()=>{loadDates().catch(()=>{})});return()=>{p.then(u=>u())}});
  let consumedFocus:string|null=null;$effect(()=>{if(!focusDate||focusDate===consumedFocus||!dates.has(focusDate))return;consumedFocus=focusDate;year=+focusDate.slice(0,4);month=+focusDate.slice(5,7);pick(focusDate)});
  async function pick(date:string){if(!dates.has(date))return;selected=date;const text=visiting?remote.get(date)?.body??null:await getDiary(date).catch(()=>null);if(selected!==date)return;try{html=text?DOMPurify.sanitize(await marked.parse(text)):null}catch{html=null}}
  function nav(delta:number){[year,month]=shiftMonth(year,month,delta)}
</script>
<section class="diary">
  <div class="side">
    <div class="cal"><header><button onclick={()=>nav(-1)}>‹</button><b>{year}. {String(month).padStart(2,'0')}</b><button onclick={()=>nav(1)}>›</button></header>
      <div class="dow">{#each ['일','월','화','수','목','금','토'] as d}<span>{d}</span>{/each}</div><div class="cells">{#each grid as c (c.date)}<button class:out={!c.inMonth} class:has={dates.has(c.date)} class:sel={selected===c.date} disabled={!dates.has(c.date)} onclick={()=>pick(c.date)}>{c.day}{#if dates.has(c.date)}<i class="dot"></i>{/if}</button>{/each}</div>
    </div>
    <!-- 내 로컬 활동이라 남의 방을 보는 동안엔 숨긴다 (홈 탭 통계와 같은 규율) -->
    {#if !visiting && selected}<DayActivity date={selected} />{/if}
  </div>
  <div class="body">
    {#if html}<article>{@html html}</article>{:else if selected}<p class="empty">이 날 일기를 불러오지 못했어요.</p>{:else}<p class="empty">{visiting?'공개된 일기를 날짜에서 선택하세요.':'도트 찍힌 날짜를 눌러 보세요.'}</p>{/if}
  </div>
</section>
<style>
  .diary{display:flex;gap:14px;padding:14px 16px;flex:1;min-height:0;box-sizing:border-box}.side{width:238px;flex-shrink:0;overflow-y:auto;padding-right:4px}.cal{background:var(--frame-bg);border-radius:var(--radius-m);box-shadow:var(--shadow-soft);padding:12px}.cal header{display:flex;justify-content:space-between;align-items:center;margin-bottom:8px}.cal header button{border:none;background:var(--pastel-lav);border-radius:var(--radius-s);cursor:pointer;font:inherit;padding:2px 9px}.dow,.cells{display:grid;grid-template-columns:repeat(7,1fr)}.dow span{text-align:center;font-size:10px;color:var(--ink-soft);padding:2px 0}.cells button{position:relative;border:none;background:none;font:inherit;font-size:11px;padding:6px 0;color:var(--ink);border-radius:var(--radius-s)}.cells button.out{color:var(--text-soft)}.cells button.has{cursor:pointer;background:var(--pastel-cream)}.cells button.sel{background:var(--accent);color:var(--accent-ink)}.dot{position:absolute;left:50%;bottom:1px;transform:translateX(-50%);width:4px;height:4px;border-radius:50%;background:var(--accent)}.body{flex:1;overflow-y:auto;min-width:0}.empty{color:var(--ink-soft)}article{background:var(--pastel-cream);border-radius:var(--radius-m);box-shadow:var(--shadow-soft);padding:18px 22px;max-width:62ch;line-height:1.75}
</style>
