<script lang="ts">
  import { lifeGoto, lifeList, type LifeListEntry } from '../api';
  import { lifeDestinations } from '../life-navigation';

  let { myLifeId, currentLifeId }: { myLifeId: string; currentLifeId: string } = $props();
  let entries = $state<LifeListEntry[]>([]);
  let localCurrentLifeId = $state('');
  let selectedLifeId = $state('');
  let loading = $state(false);
  let moving = $state(false);
  let failed = $state(false);
  let refreshRequest = 0;

  const destinations = $derived(lifeDestinations(entries, myLifeId, localCurrentLifeId));
  const currentEntry = $derived(entries.find((entry) => entry.life_id === localCurrentLifeId));
  const currentLabel = $derived.by(() => {
    const label = localCurrentLifeId === myLifeId
      ? '내 미니홈피'
      : currentEntry ? `${currentEntry.owner_name}의 방` : '현재 미니홈피';
    return currentEntry ? `${label} · ${currentEntry.occupants}명` : label;
  });

  $effect(() => {
    localCurrentLifeId = currentLifeId;
    selectedLifeId = currentLifeId;
  });

  async function refresh(mine: string, current: string, showLoading = false) {
    if (!mine || !current) {
      entries = [];
      return;
    }
    const request = ++refreshRequest;
    if (showLoading) loading = true;
    try {
      const result = await lifeList();
      if (request === refreshRequest) entries = result.life;
    } catch {
      // 순단에는 직전 목록을 유지한다.
    } finally {
      if (request === refreshRequest) loading = false;
    }
  }

  $effect(() => {
    const mine = myLifeId;
    const current = currentLifeId;
    if (!mine || !current) return;
    refresh(mine, current, true);
    const timer = setInterval(() => refresh(mine, current), 10_000);
    return () => {
      refreshRequest++;
      clearInterval(timer);
    };
  });

  async function move() {
    const lifeId = selectedLifeId;
    if (!lifeId || moving) return;
    moving = true;
    failed = false;
    try {
      const me = await lifeGoto(lifeId);
      localCurrentLifeId = me.life_id;
      selectedLifeId = me.life_id;
      await refresh(myLifeId, me.life_id);
    } catch {
      failed = true;
      selectedLifeId = localCurrentLifeId;
    } finally {
      moving = false;
    }
  }
</script>

<section class="life-navigator" aria-labelledby="life-navigator-label">
  <label id="life-navigator-label" for="life-destination">미니홈피 이동</label>
  <div class="select-wrap">
    <select
      id="life-destination"
      bind:value={selectedLifeId}
      onchange={move}
      onfocus={() => refresh(myLifeId, localCurrentLifeId)}
      disabled={moving || loading || destinations.length === 0}
    >
      <option value={localCurrentLifeId} disabled>{moving ? '이동 중…' : currentLabel}</option>
      {#each destinations as destination (destination.lifeId)}
        <option value={destination.lifeId}>
          {destination.kind === 'home' ? '내 미니홈피' : destination.label}{destination.occupants === null ? '' : ` · ${destination.occupants}명`}
        </option>
      {/each}
    </select>
    <svg viewBox="0 0 20 20" aria-hidden="true"><path d="m6 8 4 4 4-4"/></svg>
  </div>
  <small class:visible={failed} aria-live="polite">{failed ? '방을 이동하지 못했습니다.' : ''}</small>
</section>

<style>
  .life-navigator {
    margin-top: auto;
    padding-top: 12px;
    border-top: 1px solid var(--pastel-lav);
  }
  label {
    display: block;
    margin: 0 0 6px 1px;
    color: var(--ink-soft);
    font-size: 11px;
    font-weight: 700;
    letter-spacing: .02em;
  }
  .select-wrap { position: relative; }
  select {
    box-sizing: border-box;
    width: 100%;
    height: 34px;
    appearance: none;
    border: 1px solid var(--line);
    border-radius: 7px;
    padding: 0 28px 0 9px;
    background: var(--panel2);
    color: var(--ink);
    font: inherit;
    font-size: 11px;
    cursor: pointer;
  }
  select:hover:not(:disabled) { border-color: var(--accent); }
  select:focus-visible { outline: 2px solid var(--accent); outline-offset: 1px; }
  select:disabled { color: var(--ink-soft); cursor: default; opacity: .68; }
  svg {
    position: absolute;
    top: 50%;
    right: 8px;
    width: 15px;
    height: 15px;
    transform: translateY(-50%);
    fill: none;
    stroke: var(--ink-soft);
    stroke-width: 1.6;
    stroke-linecap: round;
    stroke-linejoin: round;
    pointer-events: none;
  }
  small {
    display: block;
    min-height: 14px;
    padding: 3px 1px 0;
    color: var(--danger);
    font-size: 10px;
    line-height: 1.2;
    opacity: 0;
  }
  small.visible { opacity: 1; }
</style>
