<script lang="ts">
  import { lifeAddGuestbook, lifeDeleteGuestbook, lifeGuestbook, lifeMascotFace, type GuestbookEntry } from '../api';
  import { groupGuestbook, authorIcon } from '../guestbook';
  import { listen } from '@tauri-apps/api/event';
  import NameChip from './NameChip.svelte';
  let { lifeId, meId, myLifeId, isOwner=false }: {lifeId:string;meId:string;myLifeId:string;isOwner?:boolean}=$props();
  let entries=$state<GuestbookEntry[]>([]),text=$state(''),busy=$state(false);
  let replyTo=$state<string|null>(null),replyText=$state('');
  let threads=$derived(groupGuestbook(entries));
  // G7 — 봇 얼굴은 서버 게시본(내 봇도 예외 없이). agent_id별 1회만 받아 같은 작성자의 여러 글이
  // 요청을 나눠 쓴다. 없으면 이모지 폴백. 탭을 다시 열면 새로 받으므로 리롤도 그때 반영된다.
  // requested는 일부러 비반응 — $state에 담으면 effect가 자기 쓰기로 재실행된다.
  const requested=new Set<string>();
  let botFace=$state<Record<string,string>>({});
  $effect(()=>{for(const t of threads)for(const e of [t.entry,...t.replies]){const icon=authorIcon(e);if(icon.kind!=='bot'||requested.has(icon.agentId))continue;requested.add(icon.agentId);lifeMascotFace(icon.agentId).then(v=>{if(v)botFace={...botFace,[icon.agentId]:v}}).catch(()=>{})}});
  async function load(){entries=(await lifeGuestbook(lifeId)).entries}load();
  // N1 — 내 방 방명록을 열어 둔 동안 도착한 새 글을 즉시 반영. App이 같은 이벤트로 뱃지를
  // 읽음 처리하므로 여기서 다시 읽지 않으면 화면에 안 보인 채 읽음이 된다 (Codex 리뷰 P2).
  $effect(()=>{if(lifeId!==myLifeId)return;let un:(()=>void)|null=null;listen('guestbook:new',()=>load()).then(u=>un=u);return()=>un?.()});
  async function add(){if(!text.trim()||busy)return;busy=true;try{await lifeAddGuestbook(lifeId,text);text='';await load()}finally{busy=false}}
  async function addReply(parentId:string){if(!replyText.trim()||busy)return;busy=true;try{await lifeAddGuestbook(lifeId,replyText,parentId);replyText='';replyTo=null;await load()}finally{busy=false}}
  function toggleReply(id:string){replyTo=replyTo===id?null:id;replyText=''}
  async function remove(id:string){await lifeDeleteGuestbook(id);await load()}
</script>
{#snippet avatar(entry:GuestbookEntry)}
  {@const icon=authorIcon(entry)}
  {#if icon.kind==='bot'}
    {#if botFace[icon.agentId]}<span class="ava" style="background-image:url('data:image/png;base64,{botFace[icon.agentId]}')"></span>
    {:else}<span class="ava ava-fb">🤖</span>{/if}
  {:else}
    <span class="ava ava-mono" style="background-color:hsl({icon.hue} 55% 30%)">{icon.initial}</span>
  {/if}
{/snippet}
<section>
<form onsubmit={e=>{e.preventDefault();add()}}><input maxlength="500" bind:value={text} placeholder="왔다 간 흔적을 남겨보세요"/><button disabled={busy}>남기기</button></form>
<div class="list">
{#each threads as t (t.entry.entry_id)}
  <article>
    <header>{@render avatar(t.entry)}<b><NameChip agentId={t.entry.author_agent_id} name={t.entry.author_name} {meId} {myLifeId} currentLifeId={lifeId}/></b><time>{new Date(t.entry.created_at).toLocaleString()}</time>
      <span class="acts">
        {#if isOwner}<button onclick={()=>toggleReply(t.entry.entry_id)}>답글</button>{/if}
        {#if isOwner||t.entry.author_agent_id===meId}<button onclick={()=>remove(t.entry.entry_id)}>삭제</button>{/if}
      </span>
    </header>
    <p>{t.entry.body}</p>
    {#if t.replies.length||replyTo===t.entry.entry_id}
    <div class="replies">
      {#each t.replies as reply (reply.entry_id)}
        <article class="reply">
          <header>{@render avatar(reply)}<b><NameChip agentId={reply.author_agent_id} name={reply.author_name} {meId} {myLifeId} currentLifeId={lifeId}/></b><time>{new Date(reply.created_at).toLocaleString()}</time>
            <span class="acts">{#if isOwner||reply.author_agent_id===meId}<button onclick={()=>remove(reply.entry_id)}>삭제</button>{/if}</span>
          </header>
          <p>{reply.body}</p>
        </article>
      {/each}
      {#if replyTo===t.entry.entry_id}
        <form onsubmit={e=>{e.preventDefault();addReply(t.entry.entry_id)}}><input maxlength="500" bind:value={replyText} placeholder="답글을 남겨보세요"/><button disabled={busy}>답글 달기</button></form>
      {/if}
    </div>
    {/if}
  </article>
{:else}<p>아직 방명록이 없어요.</p>{/each}
</div>
</section>
<style>section{padding:16px;display:flex;flex-direction:column;gap:12px}form{display:flex;gap:8px}input{flex:1;padding:9px;border:1px solid var(--line);border-radius:var(--radius-s);background:var(--frame-bg);color:var(--ink)}button{border:0;border-radius:var(--radius-s);padding:7px 11px;background:var(--accent);color:var(--accent-ink);cursor:pointer}.list{display:grid;gap:8px}.list article{padding:11px 13px;background:var(--frame-bg);border-radius:var(--radius-m);box-shadow:var(--shadow-soft)}.list header{display:flex;gap:8px;align-items:center;font-size:11px}.list .ava{display:inline-block;flex:0 0 auto;width:30px;height:30px;border-radius:50%;vertical-align:middle;margin-right:5px;background-color:var(--line);background-repeat:no-repeat;background-size:cover;background-position:center}.list .ava-fb{background-size:auto;font-size:17px;line-height:30px;text-align:center}.list .ava-mono{background-size:auto;font-size:14px;font-weight:700;line-height:30px;text-align:center;color:white}.list time{color:var(--ink-soft)}.list .acts{margin-left:auto;display:flex;gap:4px}.list header button{padding:3px 7px;background:var(--accent-tint);color:var(--ink)}.list p{margin:7px 0 0}.replies{margin-top:8px;display:grid;gap:6px;border-left:2px solid var(--line);padding-left:14px}.list .reply{padding:8px 10px;background:var(--panel2);border:0;border-radius:var(--radius-s);box-shadow:none}</style>
