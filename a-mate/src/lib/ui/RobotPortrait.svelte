<script lang="ts">
  import { listen } from '@tauri-apps/api/event';
  import { getDailyCut, getMascotSeed, getSprite, lifeMascotImage, robotSpecForSeed, type DailyCut } from '../api';
  import { drawRobot, type RobotSpec } from '../robot/render';
  import { frameAt } from '../robot/anim';

  // seed 지정 시 그 시드의 로봇(예: 방문 중인 미니홈피 주인), 미지정이면 내 마스코트
  let { seed = null, agentId = null, imageVersion = null }: { seed?: string | null; agentId?: string | null; imageVersion?: string | null } = $props();
  let canvas = $state<HTMLCanvasElement | null>(null);
  // AI 스프라이트(내 캐릭터 전용 캐시) — 있으면 이미지, 없으면 절차 생성 폴백
  let sprite = $state<string | null>(null);
  // H2 — 오늘의 컷 (내 화면 전용). 있으면 sprite/canvas 대신 컷+캡션 프레임.
  let cut = $state<DailyCut | null>(null);

  $effect(() => {
    if (seed) { cut = null; return; } // 방문 초상 — 내 컷 잔상 제거 (seed 토글 시 필수)
    let un: (() => void) | null = null;
    let stale = false; // 방문 전환 뒤 도착하는 인플라이트 응답 무시
    getDailyCut().then((c) => { if (!stale) cut = c; });
    listen('daily_cut:ready', () => getDailyCut().then((c) => { if (!stale) cut = c; })).then((u) => (un = u));
    return () => { stale = true; un?.(); };
  });

  $effect(() => {
    const id = agentId, version = imageVersion;
    if (!seed) return;
    sprite = null;
    if (!id || !version) return;
    lifeMascotImage(id).then((s) => (sprite = s)).catch(() => (sprite = null));
  });

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
  {#if cut}
    <figure class="cut">
      <img src={'data:image/png;base64,' + cut.png} alt="오늘의 컷" />
      <figcaption>{cut.caption}</figcaption>
    </figure>
  {:else if sprite}
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
  /* H2 — 미니홈피 대문사진 결: 이미지 위, 감성 캡션 아래 (폴라로이드 프레임) */
  .cut { margin: 0; display: flex; flex-direction: column; gap: 6px; align-items: center; }
  .cut img { width: 116px; height: 116px; object-fit: cover; border-radius: var(--radius-s); image-rendering: pixelated; }
  .cut figcaption { font-size: 11px; color: var(--ink-soft); text-align: center; line-height: 1.35; max-width: 124px; word-break: keep-all; }
</style>
