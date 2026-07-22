<script lang="ts">
  import { updateStore, installUpdate, dismiss } from './update-store.svelte';
  import { shouldShow, bannerText } from './update-banner-helpers';

  const phase = $derived(updateStore.phase);
  const visible = $derived(shouldShow(phase));
  const busy = $derived(phase.kind === 'downloading');
  const canInstall = $derived(phase.kind === 'available');
</script>

{#if visible}
  <div class="update-banner" role="status">
    <span class="msg">{bannerText(phase)}</span>
    {#if canInstall}
      <button class="primary" onclick={installUpdate}>지금 업데이트</button>
      <button class="ghost" onclick={dismiss}>나중</button>
    {:else if !busy}
      <button class="ghost" onclick={dismiss}>닫기</button>
    {/if}
  </div>
{/if}

<style>
  .update-banner {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 12px;
    margin-bottom: 8px;
    border-left: 3px solid var(--accent);
    background: var(--surface-inset);
    color: var(--ink);
    border-radius: var(--radius-m);
    box-shadow: var(--shadow-soft);
    font-size: 13px;
  }
  .msg {
    flex: 1;
  }
  button {
    border: 1px solid var(--line);
    border-radius: var(--radius-m);
    padding: 4px 10px;
    cursor: pointer;
    font-size: 12px;
    background: transparent;
    color: var(--ink);
  }
  button.primary {
    background: var(--accent);
    color: var(--accent-ink);
    border-color: var(--accent);
  }
</style>
