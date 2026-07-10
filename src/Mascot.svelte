<script lang="ts">
  import { getCurrentWindow, PhysicalPosition, LogicalSize } from '@tauri-apps/api/window';
  import './lib/theme.css';
  import {
    emitOccasionToday, getChatterPool, getMascotSeed, getSettings, getSummary, getTodayOccasions,
    listFindings, openChatTab, setSetting,
    onDiaryReady, onNewFindings, onScanDone, onSettingsChanged,
  } from './lib/api';
  import { drawRobot, type RobotSpec } from './lib/robot/render';
  import { frameAt, resolveState, type BubbleKind } from './lib/robot/anim';
  import { adviceBubble, diaryBubble, findingBubble, occasionBubble, pickChatter, type Bubble } from './lib/robot/bubble';
  import { isDrag } from './lib/robot/drag';

  const win = getCurrentWindow();
  const BASE = { w: 160, h: 160 };
  const EXPANDED = { w: 320, h: 230 };

  let canvas = $state<HTMLCanvasElement | null>(null);
  let spec = $state<RobotSpec | null>(null);
  let bubble = $state<Bubble | null>(null);
  let chatterLevel = $state('low');
  let realtimeAdvice = $state(false);
  let lastAdviceKey: string | null = null;
  let recentChatter: string[] = []; // 최근 표시 잡담 3개 (세션-로컬, 영속화 안 함)

  // 지속 말풍선 (스펙 §6): 자동 소멸 없음 — 교체/X/본문 클릭까지 유지
  async function showBubble(b: Bubble) {
    const wasShowing = bubble !== null;
    bubble = b;
    if (!wasShowing) await expand(true);
  }
  async function closeBubble(openTab: boolean) {
    const b = bubble;
    if (!b) return;
    bubble = null;
    await expand(false);
    if (openTab) openChatTab(b.tab);
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

  async function loadSettings() {
    const s = await getSettings().catch(() => ({}) as Record<string, string>);
    chatterLevel = s['chatter_level'] ?? 'low';
    realtimeAdvice = s['realtime_advice'] === 'on';
    // 재시작 후에도 같은 조언을 반복하지 않도록 영속화된 키 복원 (집계 카드는 계속 1위로 상주함)
    lastAdviceKey = s['last_advice_key'] ?? lastAdviceKey;
  }

  // occasion: pull 단일 경로 (시작 레이스 방어 — 스펙 §6). 게이트(하루 1회)는 백엔드가 가짐.
  // pull 성공 시 chat 알림 로그를 위해 같은 이벤트명으로 재방송한다.
  async function pullOccasions() {
    const labels = await getTodayOccasions().catch(() => [] as string[]);
    if (labels.length) {
      showBubble(occasionBubble(labels));
      emitOccasionToday(labels).catch(() => {});
    }
  }

  // 트리거 배선 (스펙 §6: 새/악화 advice·다이어리·occasion·잡담만 — 스캔 요약 대사 없음)
  $effect(() => {
    const subs = [
      onNewFindings((rows) => rows.length && showBubble(findingBubble(rows))),
      onDiaryReady((date) => showBubble(diaryBubble(date))),
      onScanDone(async () => {
        pullOccasions(); // 자정 넘김 대비 — 게이트 덕에 하루 1회만 유효
        if (!realtimeAdvice) return;
        const rows = await listFindings(false).catch(() => []);
        const top = rows[0];
        if (top && top.dedup_key !== lastAdviceKey) {
          lastAdviceKey = top.dedup_key;
          setSetting('last_advice_key', top.dedup_key).catch(() => {});
          showBubble(adviceBubble(top));
        }
      }),
      onSettingsChanged(() => loadSettings()),
    ];
    return () => { subs.forEach((p) => p.then((u) => u())); };
  });

  // 마운트 시 1회 pull
  pullOccasions();

  // 잡담 타이머 — 말풍선 떠 있는 동안엔 침묵
  $effect(() => {
    let timer: ReturnType<typeof setTimeout>;
    const schedule = () => {
      const [min, max] = chatterLevel === 'normal' ? [20, 40] : [60, 120];
      const delayMin = min + Math.random() * (max - min);
      timer = setTimeout(async () => {
        const hour = new Date().getHours();
        if (chatterLevel !== 'off' && !(hour >= 1 && hour < 7) && bubble === null) {
          const [summary, pool] = await Promise.all([
            getSummary().catch(() => null),
            getChatterPool().catch(() => [] as string[]),
          ]);
          const b = pickChatter(pool, summary, recentChatter, Math.random);
          recentChatter = [...recentChatter.slice(-2), b.text];
          showBubble(b);
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
        if (bubble !== null) return; // 말풍선 확장 중 임시 좌표는 저장하지 않음
        setSetting('mascot_pos', `${payload.x},${payload.y}`).catch(() => {});
      }, 1000);
    });
    return () => { clearTimeout(t); p.then((u) => u()); };
  });

  // 클릭 vs 드래그 (스펙 §6): drag-region 대신 수동 판별 — 클릭이면 홈피 열기
  let downAt: { x: number; y: number } | null = null;
  function onPointerDown(e: PointerEvent) {
    downAt = { x: e.screenX, y: e.screenY };
  }
  function onPointerMove(e: PointerEvent) {
    if (e.buttons === 0) { downAt = null; return; }
    if (!downAt) return;
    if (isDrag(downAt.x, downAt.y, e.screenX, e.screenY)) {
      downAt = null;
      win.startDragging(); // 이후는 OS가 이동을 소유
    }
  }
  function onPointerUp() {
    if (downAt) {
      downAt = null;
      openChatTab('home');
    }
  }

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
    <div class="bubble">
      <button class="text" onclick={() => closeBubble(true)}>{bubble.text}</button>
      <button class="x" aria-label="닫기" onclick={() => closeBubble(false)}>×</button>
    </div>
  {/if}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="robot"
    onpointerdown={onPointerDown}
    onpointermove={onPointerMove}
    onpointerup={onPointerUp}
  >
    <canvas bind:this={canvas} width="128" height="128"></canvas>
  </div>
</div>

<style>
  :global(html, body) { margin: 0; background: transparent; overflow: hidden; }
  .stage { width: 100vw; height: 100vh; display: flex; flex-direction: column; justify-content: flex-end; align-items: flex-end; }
  .robot { width: 128px; height: 128px; margin: 0 16px 16px 0; cursor: pointer; touch-action: none; }
  canvas { width: 128px; height: 128px; image-rendering: pixelated; }
  .bubble {
    display: flex; align-items: flex-start; gap: 2px;
    max-width: 280px; margin: 8px 12px 0 0; padding: 9px 6px 9px 12px;
    background: var(--frame-bg); color: var(--ink);
    border-radius: var(--radius-m); box-shadow: var(--shadow-soft);
    font: 12px 'Segoe UI', 'Malgun Gothic', sans-serif;
  }
  .bubble .text {
    border: none; background: none; font: inherit; color: inherit;
    cursor: pointer; text-align: left; padding: 0;
    display: -webkit-box; -webkit-line-clamp: 3; line-clamp: 3; -webkit-box-orient: vertical; overflow: hidden;
  }
  .bubble .x {
    border: none; background: none; cursor: pointer; padding: 0 4px;
    font-size: 13px; line-height: 1; color: var(--ink-soft);
  }
  .bubble .x:hover { color: var(--ink); }
</style>
