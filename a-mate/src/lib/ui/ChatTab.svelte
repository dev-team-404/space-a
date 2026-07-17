<script lang="ts">
  import { chatSend, chatStatus, type ChatStatus } from '../api';
  import { chatState } from './chat-store.svelte';

  let status = $state<ChatStatus | null>(null);
  let draft = $state('');
  let sending = $state(false);
  let error = $state<string | null>(null);
  let listEl = $state<HTMLElement | null>(null);

  chatStatus()
    .then((s) => (status = s))
    .catch(() => (status = { configured: false, model: null }));

  // 탭 재진입(마운트)·메시지 변경 시 항상 최신 메시지로 스크롤
  $effect(() => {
    chatState.messages.length; // 반응성 추적
    scrollBottom();
  });

  async function send() {
    const text = draft.trim();
    if (!text || sending) return;
    draft = '';
    error = null;
    chatState.messages.push({ role: 'user', content: text });
    sending = true;
    scrollBottom();
    try {
      const reply = await chatSend($state.snapshot(chatState.messages));
      chatState.messages.push({ role: 'assistant', content: reply });
    } catch (e) {
      error = String(e); // 인라인 표시 — 말풍선/토스트 아님 (스펙 §9)
      draft = text; // 실패 시 입력 복원 — 재입력 불필요
      chatState.messages.pop(); // 답 못 받은 user 메시지 제거 — 히스토리 일관성(sending 가드로 항상 마지막이 이 메시지)
    } finally {
      sending = false;
      scrollBottom();
    }
  }

  function onKeydown(e: KeyboardEvent) {
    // isComposing: 한글 IME 조합 중 Enter로 전송되는 것 방지
    if (e.key === 'Enter' && !e.shiftKey && !e.isComposing) {
      e.preventDefault();
      send();
    }
  }

  function scrollBottom() {
    requestAnimationFrame(() => listEl?.scrollTo({ top: listEl.scrollHeight }));
  }
</script>

<section class="chat">
  {#if status && !status.configured}
    <div class="setup">
      <p>엔진이 아직 없어요, 주인. 다이어리와 같은 엔진을 써요.</p>
      <p>
        환경변수 <code>AGENT_MENTOR_ENGINE_URL</code>(필요 시
        <code>AGENT_MENTOR_ENGINE_KEY</code> · <code>AGENT_MENTOR_ENGINE_MODEL</code>)을
        설정하고 앱을 다시 시작하면 여기서 대화할 수 있어요.
      </p>
      <p class="fine">트랜스크립트 원문은 보내지 않아요 — 요약 수치와 코칭 지적만 참고해요.</p>
    </div>
  {:else}
    <div class="list" bind:this={listEl}>
      {#if chatState.messages.length === 0}
        <p class="hint">오늘 요약이나 코칭 지적에 대해 물어보세요. (예: “왜 playwright를 빼라는 거야?”)</p>
      {/if}
      {#each chatState.messages as m, i (i)}
        <div class="msg {m.role}">{m.content}</div>
      {/each}
      {#if sending}<div class="msg assistant pending">생각 중…</div>{/if}
      {#if error}<div class="msg fail">답장을 못 받았어요: {error}</div>{/if}
    </div>
    <div class="composer">
      <textarea
        rows="2"
        placeholder="주인, 뭐가 궁금해요? (Enter 전송 · Shift+Enter 줄바꿈)"
        bind:value={draft}
        onkeydown={onKeydown}
        disabled={sending}
      ></textarea>
      <button onclick={send} disabled={sending || !draft.trim()}>보내기</button>
    </div>
  {/if}
</section>

<style>
  .chat { flex: 1; display: flex; flex-direction: column; min-height: 0; padding: 14px 16px; gap: 10px; }
  .setup {
    margin: auto; max-width: 420px; font-size: 13px; color: var(--ink);
    background: var(--frame-bg); border-radius: var(--radius-m); box-shadow: var(--shadow-soft);
    padding: 18px 20px;
  }
  .setup .fine { color: var(--ink-soft); font-size: 12px; }
  .setup code { background: var(--pastel-lav); border-radius: var(--radius-s); padding: 1px 4px; }
  .list { flex: 1; overflow-y: auto; display: flex; flex-direction: column; gap: 8px; padding-right: 4px; }
  .hint { color: var(--ink-soft); font-size: 12px; margin: auto; }
  .msg {
    max-width: 78%; padding: 8px 12px; font-size: 13px; white-space: pre-wrap;
    border-radius: var(--radius-m); box-shadow: var(--shadow-soft); line-height: 1.5;
  }
  .msg.user { align-self: flex-end; background: var(--pastel-lav); }
  .msg.assistant { align-self: flex-start; background: var(--frame-bg); }
  .msg.pending { color: var(--ink-soft); animation: blink 1.2s ease-in-out infinite; }
  .msg.fail { align-self: flex-start; background: var(--pastel-coral); }
  @keyframes blink { 50% { opacity: 0.4; } }
  .composer { display: flex; gap: 8px; align-items: flex-end; }
  .composer textarea {
    flex: 1; resize: none; font: inherit; font-size: 13px; color: var(--ink);
    border: 1px solid var(--pastel-lav); border-radius: var(--radius-s);
    padding: 8px 10px; background: var(--frame-bg); box-sizing: border-box;
  }
  .composer button {
    border: none; cursor: pointer; font: inherit; font-size: 13px;
    background: var(--pastel-lav); color: var(--ink);
    border-radius: var(--radius-s); padding: 9px 16px;
  }
  .composer button:disabled { opacity: 0.6; cursor: default; }
</style>
