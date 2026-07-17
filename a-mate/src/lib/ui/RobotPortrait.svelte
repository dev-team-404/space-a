<script lang="ts">
  import { getMascotSeed, robotSpecForSeed } from '../api';
  import { drawRobot, type RobotSpec } from '../robot/render';
  import { frameAt } from '../robot/anim';

  // seed 지정 시 그 시드의 로봇(예: 방문 중인 미니홈피 주인), 미지정이면 내 마스코트
  let { seed = null }: { seed?: string | null } = $props();
  let canvas = $state<HTMLCanvasElement | null>(null);

  $effect(() => {
    if (!canvas) return;
    const ctx = canvas.getContext('2d')!;
    const spec = seed ? robotSpecForSeed(seed) : getMascotSeed();
    spec.then((s: RobotSpec) => drawRobot(ctx, s, frameAt('idle', 300)));
  });
</script>

<div class="portrait">
  <canvas bind:this={canvas} width="128" height="128"></canvas>
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
</style>
