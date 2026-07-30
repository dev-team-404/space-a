<script lang="ts">
  import { listen } from '@tauri-apps/api/event';
  import { getDailyCut, getMascotSeed, getSprite, lifeDailyCut, lifeMascotImage, robotSpecForSeed, type DailyCut } from '../api';
  import { drawRobot, type RobotSpec } from '../robot/render';
  import { frameAt } from '../robot/anim';

  // seed 지정 시 그 시드의 로봇(예: 방문 중인 미니홈피 주인), 미지정이면 내 마스코트
  let { seed = null, agentId = null, imageVersion = null, cutAgentId = null, cutVersion = null }: {
    seed?: string | null; agentId?: string | null; imageVersion?: string | null;
    cutAgentId?: string | null; cutVersion?: string | null;
  } = $props();
  let canvas = $state<HTMLCanvasElement | null>(null);
  // AI 스프라이트(내 캐릭터 전용 캐시) — 있으면 이미지, 없으면 절차 생성 폴백
  let sprite = $state<string | null>(null);
  // H2 — 오늘의 컷 (내 화면 전용). 있으면 sprite/canvas 대신 컷+캡션 프레임.
  let cut = $state<DailyCut | null>(null);
  // O1 — 방문 중인 방 주인의 대문사진 base64. 서버 게시분이라 캡션은 App의 카드가 담당한다.
  let visitCut = $state<string | null>(null);
  const cutPng = $derived(seed ? visitCut : cut?.png ?? null);

  $effect(() => {
    if (seed) { cut = null; return; } // 방문 초상 — 내 컷 잔상 제거 (seed 토글 시 필수)
    let un: (() => void) | null = null;
    let stale = false; // 방문 전환 뒤 도착하는 인플라이트 응답 무시
    getDailyCut().then((c) => { if (!stale) cut = c; });
    listen('daily_cut:ready', () => getDailyCut().then((c) => { if (!stale) cut = c; })).then((u) => (un = u));
    return () => { stale = true; un?.(); };
  });

  // O1 — 방문 컷. cutVersion(sha256)이 무효화 키 (마스코트의 imageVersion과 같은 역할).
  // 없으면 아무것도 세팅하지 않고 마스코트 이미지 → 시드 절차 생성으로 폴백한다.
  $effect(() => {
    const id = cutAgentId, version = cutVersion;
    if (!seed) { visitCut = null; return; }
    visitCut = null;
    if (!id || !version) return;
    let stale = false;
    lifeDailyCut(id).then((png) => { if (!stale) visitCut = png; }).catch(() => {});
    return () => { stale = true; };
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

<!-- 컷 = 자체 배경을 가진 사진이라 풀블리드, 폴백 캐릭터(투명 배경)만 민트 여백 유지 -->
<div class="portrait" class:full={!!cutPng}>
  {#if cutPng}
    <!-- 캡션은 App의 "오늘의 한마디" 카드가 표시 — 초상은 이미지만 (공간 절약) -->
    <img class="cut" src={'data:image/png;base64,' + cutPng} alt="오늘의 대문사진" />
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
  /* H2 — 미니홈피 대문사진: 박스를 꽉 채우는 풀블리드 (감성 글귀는 "오늘의 한마디" 카드가 담당) */
  .portrait.full { padding: 0; overflow: hidden; }
  .cut { display: block; width: 100%; aspect-ratio: 1 / 1; height: auto; object-fit: cover; image-rendering: pixelated; }
</style>
