<script lang="ts">
  import { getMascotSeed } from '../api';
  import { drawRobot, type RobotSpec } from '../robot/render';
  import { frameAt } from '../robot/anim';

  let canvas = $state<HTMLCanvasElement | null>(null);

  $effect(() => {
    if (!canvas) return;
    const ctx = canvas.getContext('2d')!;
    getMascotSeed().then((spec: RobotSpec) => drawRobot(ctx, spec, frameAt('idle', 300)));
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
