<script lang="ts">
  import { getCurrentWindow, PhysicalPosition, LogicalSize } from '@tauri-apps/api/window';
  import {
    getMascotSeed, getSummary, getSettings, setSetting,
    onDiaryReady, onNewFindings, onOccasionToday, openChatTab,
    onScanDone, onSettingsChanged,
  } from './lib/api';
  import { drawRobot, type RobotSpec } from './lib/robot/render';
  import { frameAt, resolveState, type BubbleKind } from './lib/robot/anim';
  import { BubbleQueue, chatterBubble, diaryBubble, findingBubble, occasionBubble, scanBubble, type Bubble } from './lib/robot/bubble';

  const win = getCurrentWindow();
  const BASE = { w: 160, h: 160 };
  const EXPANDED = { w: 320, h: 230 };

  let canvas = $state<HTMLCanvasElement | null>(null);
  let spec = $state<RobotSpec | null>(null);
  let bubble = $state<Bubble | null>(null);
  let chatterLevel = $state('low');
  let realtimeAdvice = $state(false);

  const queue = new BubbleQueue();
  let showing = false;

  async function pump() {
    if (showing) return;
    const b = queue.next();
    if (!b) return;
    showing = true;
    bubble = b;
    await expand(true);
    setTimeout(async () => {
      bubble = null;
      await expand(false);
      showing = false;
      setTimeout(pump, 500);
    }, 6000);
  }

  async function expand(on: boolean) {
    // 캐릭터(창 우하단 고정)가 화면상 제자리를 지키도록 위치 보정 (델타는 물리 픽셀로 환산)
    const scale = await win.scaleFactor();
    const pos = await win.outerPosition();
    const dw = Math.round((EXPANDED.w - BASE.w) * scale);
    const dh = Math.round((EXPANDED.h - BASE.h) * scale);
    if (on) {
      await win.setPosition(new PhysicalPosition(pos.x - dw, pos.y - dh));
      await win.setSize(new LogicalSize(EXPANDED.w, EXPANDED.h));
    } else {
      await win.setSize(new LogicalSize(BASE.w, BASE.h));
      await win.setPosition(new PhysicalPosition(pos.x + dw, pos.y + dh));
    }
  }

  function enqueue(b: Bubble) {
    queue.push(b);
    pump();
  }

  async function loadSettings() {
    const s = await getSettings().catch(() => ({} as Record<string, string>));
    chatterLevel = s['chatter_level'] ?? 'low';
    realtimeAdvice = s['realtime_advice'] === 'on';
  }

  // 트리거 배선
  $effect(() => {
    const subs = [
      onNewFindings((rows) => rows.length && enqueue(findingBubble(rows))),
      onDiaryReady((date) => enqueue(diaryBubble(date))),
      onOccasionToday((labels) => labels.length && enqueue(occasionBubble(labels))),
      onScanDone(async () => {
        if (!realtimeAdvice) return;
        const summary = await getSummary().catch(() => null);
        enqueue(scanBubble(summary));
      }),
      onSettingsChanged(() => loadSettings()),
    ];
    return () => { subs.forEach((p) => p.then((u) => u())); };
  });

  // 잡담 타이머
  $effect(() => {
    let timer: ReturnType<typeof setTimeout>;
    const schedule = () => {
      const [min, max] = chatterLevel === 'normal' ? [20, 40] : [60, 120];
      const delayMin = min + Math.random() * (max - min);
      timer = setTimeout(async () => {
        const hour = new Date().getHours();
        if (chatterLevel !== 'off' && !(hour >= 1 && hour < 7) && !showing) {
          const summary = await getSummary().catch(() => null);
          enqueue(chatterBubble(Math.floor(Math.random() * 10), summary));
        }
        schedule();
      }, delayMin * 60_000);
    };
    schedule();
    return () => clearTimeout(timer);
  });

  // 위치 저장 (이동 1초 디바운스)
  $effect(() => {
    let t: ReturnType<typeof setTimeout>;
    const p = win.onMoved(({ payload }) => {
      clearTimeout(t);
      t = setTimeout(() => {
        if (showing) return; // 말풍선 확장 중 임시 좌표는 저장하지 않음 (복원 시 onMoved가 다시 온다)
        setSetting('mascot_pos', `${payload.x},${payload.y}`).catch(() => {});
      }, 1000);
    });
    return () => { clearTimeout(t); p.then((u) => u()); };
  });

  // 렌더 루프
  $effect(() => {
    if (!canvas || !spec) return;
    const ctx = canvas.getContext('2d')!;
    let raf = 0;
    const loop = (t: number) => {
      const state = resolveState({ bubbleKind: (bubble?.kind ?? null) as BubbleKind | null, hour: new Date().getHours() });
      const f = frameAt(state, t);
      drawRobot(ctx, spec!, f);
      raf = requestAnimationFrame(loop);
    };
    raf = requestAnimationFrame(loop);
    return () => cancelAnimationFrame(raf);
  });

  getMascotSeed().then((s) => (spec = s));
  loadSettings();
</script>

<div class="stage" class:expanded={bubble !== null}>
  {#if bubble}
    <button class="bubble" onclick={() => { openChatTab(bubble!.tab); }}>
      {bubble.text}
    </button>
  {/if}
  <div class="robot" data-tauri-drag-region>
    <canvas bind:this={canvas} width="128" height="128" data-tauri-drag-region></canvas>
  </div>
</div>

<style>
  :global(html, body) { margin: 0; background: transparent; overflow: hidden; }
  .stage { width: 100vw; height: 100vh; display: flex; flex-direction: column; justify-content: flex-end; align-items: flex-end; }
  .robot { width: 128px; height: 128px; margin: 0 16px 16px 0; cursor: grab; }
  canvas { width: 128px; height: 128px; image-rendering: pixelated; }
  .bubble {
    max-width: 280px; margin: 8px 12px 0 0; padding: 8px 10px;
    background: #fffdf5; color: #33325a; border: 3px solid #33325a;
    box-shadow: 3px 3px 0 #33325a; font: 12px 'Galmuri11', 'DungGeunMo', monospace;
    cursor: pointer; text-align: left;
    display: -webkit-box; -webkit-line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden;
  }
</style>
