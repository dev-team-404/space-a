<script lang="ts">
  import { lifeGoto, lifePeople, type LifePerson } from '../api';
  import { getPeople, resolveVisitTarget } from '../people';
  let { agentId, name, meId, myLifeId, currentLifeId }: {
    agentId: string; name: string; meId: string; myLifeId: string; currentLifeId: string;
  } = $props();
  let people = $state<LifePerson[]>([]);
  $effect(() => { let alive = true; getPeople(lifePeople).then((p) => { if (alive) people = p; }); return () => { alive = false; }; });
  const target = $derived(resolveVisitTarget(agentId, name, { meId, myLifeId, currentLifeId, people }));
  let open = $state(false), busy = $state(false);
  let root = $state<HTMLElement | null>(null);
  // 바깥 클릭 닫기 — LifeView .scene의 contain/transform이 fixed 백드롭을 어긋나게 하므로 문서 리스너 방식
  $effect(() => {
    if (!open) return;
    const onDown = (e: PointerEvent) => { if (root && !root.contains(e.target as Node)) open = false; };
    document.addEventListener('pointerdown', onDown, true);
    return () => document.removeEventListener('pointerdown', onDown, true);
  });
  async function go() {
    if (!target || busy) return;
    busy = true;
    try { await lifeGoto(target.lifeId); } catch { /* 이동 실패 — 조용히 닫기 (Mascot gotoLife 선례) */ }
    finally { busy = false; open = false; }
  }
</script>

{#if target}
  <span class="chip" bind:this={root}>
    <button class="nm" onclick={() => (open = !open)}>{name}</button>
    {#if open}
      <span class="pop"><button class="go" disabled={busy} onclick={go}>{target.label} →</button></span>
    {/if}
  </span>
{:else}
  <span class="plain">{name}</span>
{/if}

<style>
  /* pointer-events:auto — LifeView .agent(none) 안에서 클릭 가능한 이름만 재활성화 */
  .chip { position: relative; display: inline-block; pointer-events: auto; }
  .nm {
    font: inherit; font-weight: inherit; color: inherit;
    background: none; border: 0; padding: 0; cursor: pointer;
    text-decoration: underline dotted; text-underline-offset: 2px;
  }
  .nm:hover { color: var(--accent-strong); }
  .plain { font-weight: inherit; }
  /* left:0 — 가운데 정렬은 좌측 끝 이름에서 스크롤 컨테이너 밖으로 잘림(방명록) → 이름 왼쪽 기준으로 안쪽 전개 */
  .pop {
    position: absolute; left: 0; bottom: calc(100% + 5px);
    z-index: 1001; white-space: nowrap;
    background: var(--panel2); border: 1px solid var(--line); border-radius: 8px;
    box-shadow: var(--shadow-soft); padding: 4px;
  }
  .go {
    font: inherit; border: 0; border-radius: 6px; padding: 5px 9px; cursor: pointer;
    background: var(--accent); color: var(--accent-ink);
  }
  .go:disabled { opacity: 0.6; cursor: default; }
</style>
