<script lang="ts">
  import type { Status } from './status';
  let { status }: { status: Status } = $props();
</script>

{#if status.text}<p class="status" data-kind={status.kind}>{status.text}</p>{/if}

<style>
  .status{margin:10px 0 0;font-size:12px;padding:7px 11px;border-radius:8px;background:var(--surface-inset);color:var(--text-soft);word-break:break-all}
  .status[data-kind=ok]{background:var(--lav-surface);color:var(--lav-ink)}
  .status[data-kind=err]{color:var(--danger)}
</style>
