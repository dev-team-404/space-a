<script lang="ts">
  import { lifeAddGuestbook, lifeDeleteGuestbook, lifeGuestbook, type GuestbookEntry } from '../api';
  let { lifeId, meId, isOwner=false }: {lifeId:string;meId:string;isOwner?:boolean}=$props();
  let entries=$state<GuestbookEntry[]>([]),text=$state(''),busy=$state(false);
  async function load(){entries=(await lifeGuestbook(lifeId)).entries}load();
  async function add(){if(!text.trim()||busy)return;busy=true;try{await lifeAddGuestbook(lifeId,text);text='';await load()}finally{busy=false}}
  async function remove(id:string){await lifeDeleteGuestbook(id);await load()}
</script>
<section><form onsubmit={e=>{e.preventDefault();add()}}><input maxlength="500" bind:value={text} placeholder="왔다 간 흔적을 남겨보세요"/><button disabled={busy}>남기기</button></form><div class="list">{#each entries as entry (entry.entry_id)}<article><header><b>{entry.author_name}</b><time>{new Date(entry.created_at).toLocaleString()}</time>{#if isOwner||entry.author_agent_id===meId}<button onclick={()=>remove(entry.entry_id)}>삭제</button>{/if}</header><p>{entry.body}</p></article>{:else}<p>아직 방명록이 없어요.</p>{/each}</div></section>
<style>section{padding:16px;display:flex;flex-direction:column;gap:12px}form{display:flex;gap:8px}input{flex:1;padding:9px;border:1px solid var(--pastel-lav);border-radius:8px}button{border:0;border-radius:8px;padding:7px 11px;background:var(--accent);color:var(--accent-ink);cursor:pointer}.list{display:grid;gap:8px}.list article{padding:11px 13px;background:var(--pastel-cream);border-radius:10px}.list header{display:flex;gap:8px;align-items:center;font-size:11px}.list time{color:var(--ink-soft)}.list header button{margin-left:auto;padding:3px 7px;background:var(--pastel-lav);color:var(--ink)}.list p{margin:7px 0 0}</style>
