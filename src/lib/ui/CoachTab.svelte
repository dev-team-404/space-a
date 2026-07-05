<script lang="ts">
  import { listFindings, onNewFindings, type Finding } from '../api';

  let { focusKey = null }: { focusKey?: string | null } = $props();
  void focusKey;

  let findings = $state<Finding[]>([]);
  let open = $state<string | null>(null);

  async function refresh() {
    findings = await listFindings();
  }
  refresh();
  $effect(() => {
    const p = onNewFindings(() => refresh());
    return () => { p.then((u) => u()); };
  });

  const icon = (s: Finding['severity']) => (s === 'warn' ? '⚠' : s === 'suggest' ? '💡' : 'ℹ');
</script>

<section class="coach">
  {#if findings.length === 0}
    <p>지적할 게 없어요, 주인. 완벽해요!</p>
  {:else}
    {#each findings as f (f.dedup_key)}
      <article class="finding" class:warn={f.severity === 'warn'}>
        <button class="head" onclick={() => (open = open === f.dedup_key ? null : f.dedup_key)}>
          <span>{icon(f.severity)} [{f.rule_id}] {f.scope_kind}: {f.scope_ref}</span>
          <span class="save">~{f.est_tokens_saved.toLocaleString()} tok</span>
        </button>
        {#if open === f.dedup_key}
          <div class="detail">
            <div>범위: {f.scope_host ?? '-'} / {f.scope_project ?? '-'} · {f.occurrences}회 관측</div>
            <pre>{JSON.stringify(f.evidence, null, 2)}</pre>
            {#if f.prescription}
              <div class="rx">처방: {f.prescription.kind}</div>
              <pre>{JSON.stringify(f.prescription.payload, null, 2)}</pre>
            {/if}
          </div>
        {/if}
      </article>
    {/each}
  {/if}
</section>

<style>
  .coach { padding: 16px; overflow-y: auto; }
  .finding { border: 3px solid #33325a; background: #fffdf5; margin-bottom: 10px; box-shadow: 4px 4px 0 #c9c3dd; }
  .finding.warn { background: #fde8e0; }
  .head {
    width: 100%; display: flex; justify-content: space-between; gap: 8px;
    border: none; background: none; font: inherit; padding: 10px 12px; cursor: pointer; text-align: left;
  }
  .save { white-space: nowrap; }
  .detail { border-top: 2px dashed #33325a; padding: 10px 12px; font-size: 12px; }
  pre { background: #efece2; padding: 8px; overflow-x: auto; }
  .rx { margin-top: 6px; font-weight: bold; }
</style>
