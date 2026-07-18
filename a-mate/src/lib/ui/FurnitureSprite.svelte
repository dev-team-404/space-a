<script lang="ts">
  import { FURNITURE_BY_ID, type Rotation } from '../interior/catalog';
  let { assetId, rotation = 0, label = '', room = false }: { assetId: string; rotation?: Rotation; label?: string; room?: boolean } = $props();
  const item = $derived(FURNITURE_BY_ID.get(assetId));
  const src = $derived(item?.sprites[rotation] ?? '');
  const mirrored = $derived(item?.render.mirrorX[rotation] ?? false);
  const shearY = $derived(item?.render.shearY[rotation] ?? 0);
</script>

<span class="sprite" class:room class:mirrored style={`--shear-y:${shearY}`}>
  {#if src}<img {src} alt={label} draggable="false" />{/if}
</span>

<style>
  .sprite{position:relative;display:flex;width:100%;height:100%;min-width:0;align-items:flex-end;justify-content:center;overflow:visible}
  img{display:block;max-width:100%;max-height:100%;width:auto;height:auto;object-fit:contain;image-rendering:pixelated;user-select:none}
  .sprite.room{display:block;height:auto;transform:matrix(1,var(--shear-y),0,1,0,0);transform-origin:0 0}
  .sprite.room img{width:100%;height:auto;max-width:none;max-height:none;object-fit:initial}
  .sprite.mirrored img{transform:scaleX(-1)}
</style>
