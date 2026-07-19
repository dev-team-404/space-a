<script lang="ts">
  import { listen } from '@tauri-apps/api/event';
  import { getMascotSeed, getSprite, robotSpecForSeed } from '../api';
  import { drawRobot, type RobotSpec } from '../robot/render';
  import { frameAt } from '../robot/anim';

  // seed 지정 시 그 시드의 로봇(예: 방문 중인 미니홈피 주인), 미지정이면 내 마스코트
  let { seed = null }: { seed?: string | null } = $props();
  let canvas = $state<HTMLCanvasElement | null>(null);
  // AI 스프라이트(내 캐릭터 전용 캐시) — 있으면 이미지, 없으면 절차 생성 폴백
  let sprite = $state<string | null>(null);

  $effect(() => {
    if (seed) return; // 남의 초상은 시드 절차 생성만
    let un: (() => void) | null = null;
    getSprite().then((s) => (sprite = s));
    listen('sprite:ready', () => getSprite().then((s) => (sprite = s))).then((u) => (un = u));
    return () => un?.();
  });

  $effect(() => {
    if (!canvas) return;
    const ctx = canvas.getContext('2d')!;
    const spec = seed ? robotSpecForSeed(seed) : getMascotSeed();
    spec.then((s: RobotSpec) => drawRobot(ctx, s, frameAt('idle', 300)));
  });
</script>

<div class="portrait">
  {#if !seed && sprite}
    <img class="sprite" src={'data:image/png;base64,' + sprite} alt="내 캐릭터" />
  {:else}
    <canvas bind:this={canvas} width="128" height="128"></canvas>
  {/if}
</div>

<style>
  .portrait {
    background: var(--pastel-mint);
    border-radius: var(--radius-m);
    box-shadow: var(--shadow-soft);
    padding: 10px;
    display: flex;
    justify-content: center;
  }
  canvas { width: 96px; height: 96px; image-rendering: pixelated; }
  .sprite { width: 96px; height: 96px; object-fit: contain; }
</style>
