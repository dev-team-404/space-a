<script lang="ts">
  import { listen } from '@tauri-apps/api/event';
  import { getMascotSeed, getSprite } from '../api';
  import { drawRobot, type RobotSpec } from '../robot/render';
  import { frameAt, type MascotState } from '../robot/anim';

  let { advice }: { advice: string | null } = $props();

  let canvas = $state<HTMLCanvasElement | null>(null);
  let spec = $state<RobotSpec | null>(null);
  let sprite = $state<string | null>(null);
  $effect(() => {
    let un: (() => void) | null = null;
    getSprite().then((v) => (sprite = v));
    listen('sprite:ready', () => getSprite().then((v) => (sprite = v))).then((u) => (un = u));
    return () => un?.();
  });
  let mode = $state<MascotState>('idle');
  let line = $state('오늘도 화이팅이에요, 주인!');

  const CHATTER = [
    '오늘도 화이팅이에요, 주인!',
    '토큰은 아끼라고 있는 거예요',
    '주인, 물 한 잔 마시고 해요',
    '커밋은 자주, 후회는 짧게',
    '이 방 아늑하죠? 제 방이에요',
  ];
  const HAPPY = ['히히, 간지러워요!', '주인 최고!', '또 눌러 봐요!'];
  const pick = (arr: string[]) => arr[Math.floor(Math.random() * arr.length)];

  // 대사 순환: 기분/advice/잡담 (스펙 §2)
  $effect(() => {
    const t = setInterval(() => {
      if (mode !== 'idle') return;
      line = advice && Math.random() < 0.5 ? advice : pick(CHATTER);
    }, 45_000);
    return () => clearInterval(t);
  });

  function poke() {
    mode = 'happy';
    line = pick(HAPPY);
    setTimeout(() => (mode = 'idle'), 2500);
  }

  $effect(() => {
    if (!canvas || !spec) return;
    const ctx = canvas.getContext('2d')!;
    let raf = 0;
    const loop = (t: number) => {
      drawRobot(ctx, spec!, frameAt(mode, t));
      raf = requestAnimationFrame(loop);
    };
    raf = requestAnimationFrame(loop);
    return () => cancelAnimationFrame(raf);
  });

  getMascotSeed().then((s) => (spec = s));
</script>

<div class="room">
  <div class="window"></div>
  <div class="plant">🪴</div>
  <div class="bubble">{line}</div>
  <button class="robot" onclick={poke} aria-label="로봇 쓰다듬기">
    {#if sprite}
      <img class="spr" class:happy={mode === 'happy'} src={'data:image/png;base64,' + sprite} alt="마스코트" draggable="false" />
    {:else}
      <canvas bind:this={canvas} width="128" height="128"></canvas>
    {/if}
  </button>
  <div class="rug"></div>
  <div class="floor"></div>
</div>

<style>
  .room {
    position: relative; height: 190px; overflow: hidden;
    background: linear-gradient(180deg, var(--pastel-lav) 0%, #e9e3f8 68%, transparent 68%);
    border-radius: var(--radius-m); box-shadow: var(--shadow-soft);
  }
  .floor {
    position: absolute; left: 0; right: 0; bottom: 0; height: 32%;
    background: var(--pastel-cream);
  }
  .rug {
    position: absolute; left: 50%; bottom: 8px; transform: translateX(-50%);
    width: 190px; height: 40px; border-radius: 50%;
    background: var(--pastel-mint); z-index: 1;
  }
  .window {
    position: absolute; left: 22px; top: 18px; width: 64px; height: 52px;
    background: var(--pastel-mint); border-radius: var(--radius-s);
    box-shadow: inset 0 0 0 4px var(--frame-bg);
  }
  .plant { position: absolute; right: 20px; bottom: 46px; font-size: 26px; z-index: 2; }
  .robot {
    position: absolute; left: 50%; bottom: 20px; transform: translateX(-50%);
    border: none; background: none; padding: 0; cursor: pointer; z-index: 2;
  }
  canvas { width: 112px; height: 112px; image-rendering: pixelated; display: block; }
  .spr { width: 112px; height: 112px; object-fit: contain; display: block; animation: mr-bounce 2.6s ease-in-out infinite; }
  .spr.happy { animation: mr-hop 0.5s ease-in-out 3; }
  @keyframes mr-bounce { 0%, 100% { transform: translateY(0); } 50% { transform: translateY(-3px); } }
  @keyframes mr-hop { 0%, 100% { transform: translateY(0) scale(1); } 50% { transform: translateY(-8px) scale(1.04); } }
  .bubble {
    position: absolute; left: 50%; top: 12px; transform: translateX(-50%);
    max-width: 65%; z-index: 3;
    background: var(--frame-bg); color: var(--ink);
    border-radius: var(--radius-m); box-shadow: var(--shadow-soft);
    padding: 7px 12px; font-size: 12px; text-align: center;
    display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden;
  }
</style>
