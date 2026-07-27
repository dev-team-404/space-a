<script lang="ts">
  import { lifeAddGuestbook, lifeDeleteGuestbook, lifeGuestbook, getSprite, type GuestbookEntry } from '../api';
  import { groupGuestbook, showOwnerAvatar } from '../guestbook';
  let { lifeId, meId, isOwner=false }: {lifeId:string;meId:string;isOwner?:boolean}=$props();
  let entries=$state<GuestbookEntry[]>([]),text=$state(''),busy=$state(false);
  let replyTo=$state<string|null>(null),replyText=$state('');
  let ownSprite=$state<string|null>(null); getSprite().then(s=>ownSprite=s);
  let threads=$derived(groupGuestbook(entries));
  async function load(){entries=(await lifeGuestbook(lifeId)).entries}load();
  async function add(){if(!text.trim()||busy)return;busy=true;try{await lifeAddGuestbook(lifeId,text);text='';await load()}finally{busy=false}}
  async function addReply(parentId:string){if(!replyText.trim()||busy)return;busy=true;try{await lifeAddGuestbook(lifeId,replyText,parentId);replyText='';replyTo=null;await load()}finally{busy=false}}
  function toggleReply(id:string){replyTo=replyTo===id?null:id;replyText=''}
  async function remove(id:string){await lifeDeleteGuestbook(id);await load()}
</script>
<section>
<form onsubmit={e=>{e.preventDefault();add()}}><input maxlength="500" bind:value={text} placeholder="왔다 간 흔적을 남겨보세요"/><button disabled={busy}>남기기</button></form>
<div class="list">
{#each threads as t (t.entry.entry_id)}
  <article>
    <header>{#if showOwnerAvatar(t.entry,meId,isOwner)}{#if ownSprite}<span class="ava" style="background-image:url('data:image/png;base64,{ownSprite}')"></span>{:else}<span class="ava ava-fb">🤖</span>{/if}{/if}<b>{t.entry.author_name}</b><time>{new Date(t.entry.created_at).toLocaleString()}</time>
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
          <header>{#if showOwnerAvatar(reply,meId,isOwner)}{#if ownSprite}<span class="ava" style="background-image:url('data:image/png;base64,{ownSprite}')"></span>{:else}<span class="ava ava-fb">🤖</span>{/if}{/if}<b>{reply.author_name}</b><time>{new Date(reply.created_at).toLocaleString()}</time>
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
<style>section{padding:16px;display:flex;flex-direction:column;gap:12px}form{display:flex;gap:8px}input{flex:1;padding:9px;border:1px solid var(--pastel-lav);border-radius:8px}button{border:0;border-radius:8px;padding:7px 11px;background:var(--accent);color:var(--accent-ink);cursor:pointer}.list{display:grid;gap:8px}.list article{padding:11px 13px;background:var(--pastel-cream);border-radius:10px}.list header{display:flex;gap:8px;align-items:center;font-size:11px}.list .ava{display:inline-block;flex:0 0 auto;width:30px;height:30px;border-radius:50%;vertical-align:middle;margin-right:5px;background-color:var(--pastel-lav);background-repeat:no-repeat;background-size:180%;background-position:50% 14%}.list .ava-fb{background-size:auto;font-size:17px;line-height:30px;text-align:center}.list time{color:var(--ink-soft)}.list .acts{margin-left:auto;display:flex;gap:4px}.list header button{padding:3px 7px;background:var(--pastel-lav);color:var(--ink)}.list p{margin:7px 0 0}.replies{margin-top:8px;display:grid;gap:6px;border-left:2px solid var(--pastel-lav);padding-left:10px}.list .reply{padding:8px 10px;background:transparent;border:1px solid var(--pastel-lav);border-radius:8px}</style>
