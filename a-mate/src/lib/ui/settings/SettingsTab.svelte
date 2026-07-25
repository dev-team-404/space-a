<script lang="ts">
  import { DEFAULT_GROUP, SETTINGS_GROUPS, type SettingsGroup } from './groups';
  import ConnectionGroup from './ConnectionGroup.svelte';
  import LookGroup from './LookGroup.svelte';
  import MeGroup from './MeGroup.svelte';
  import PrivacyGroup from './PrivacyGroup.svelte';

  let { group = DEFAULT_GROUP }: { group?: SettingsGroup } = $props();

  let active = $state<SettingsGroup>(group);
  // 딥링크(트레이·마스코트)가 이미 열린 설정 탭의 그룹을 바꿀 수 있어야 한다.
  $effect(() => { active = group; });
</script>

<div class="tab">
  <nav class="seg groups">
    {#each SETTINGS_GROUPS as g (g.id)}
      <button class:active={active===g.id} onclick={()=>active=g.id}>{g.label}</button>
    {/each}
  </nav>
  <div class="group">
    {#if active === 'conn'}<ConnectionGroup/>
    {:else if active === 'me'}<MeGroup/>
    {:else if active === 'privacy'}<PrivacyGroup/>
    {:else}<LookGroup/>{/if}
  </div>
</div>

<style>
  .tab{padding:18px}
  .seg.groups{display:inline-flex;border:1px solid var(--line);border-radius:9px;overflow:hidden}
  .seg.groups button{border:0;border-right:1px solid var(--line);border-radius:0;background:transparent;color:var(--text);padding:7px 18px;cursor:pointer;font:inherit}
  .seg.groups button:last-child{border-right:0}
  .seg.groups button.active{background:var(--accent);color:var(--accent-ink);font-weight:700}
  .group{margin-top:16px}
  /* 공용 섹션 스타일 — 각 그룹이 <section><h2>…를 직접 쓰고 여기서 모양을 준다 */
  .group :global(h2){margin:0 0 5px;font-size:16px}
  .group :global(.hint){margin:0;color:var(--text-soft);font-size:12px}
  .group :global(section + section){border-top:1px solid var(--line);margin-top:20px;padding-top:20px}
</style>
