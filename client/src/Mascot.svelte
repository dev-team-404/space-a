<script lang="ts">
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import './lib/theme.css';
  import {
    emitOccasionToday, getChatterPool, getMascotSeed, getSettings, getSummary, getTodayOccasions,
    hubSettingsGet, listFindings, mascotSetExpanded, openChatTab, openSettingsWindow,
    roomGoto, roomView, roomsList, setSetting,
    onDiaryReady, onNewFindings, onScanDone, onSettingsChanged,
    type RoomListEntry,
  } from './lib/api';
  import { drawRobot, type RobotSpec } from './lib/robot/render';
  import { frameAt, resolveState, type BubbleKind } from './lib/robot/anim';
  import { adviceBubble, diaryBubble, findingBubble, occasionBubble, pickChatter, type Bubble } from './lib/robot/bubble';
  import { isDrag } from './lib/robot/drag';

  const win = getCurrentWindow();

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
    if (openTab) openChatTab(b.tab, b.target);
  }

  async function expand(on: boolean) {
    // 창 크기는 상시 확장 크기로 고정(리사이즈 깜빡임 원천 차단) — 백엔드에 열림
    // 상태만 알려, 접힘 시 로봇 밖 투명 여백의 클릭 통과 여부를 전환한다
    await mascotSetExpanded(on).catch(() => {});
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

  // 방 이동 팝오버 (docs/design/room-visit.md §3): 우클릭 = 메뉴, 클릭 = 홈피(기존)
  let roomMenu = $state<RoomListEntry[] | null>(null); // null = 닫힘
  let myRoomId = $state('');
  let curRoomId = $state(''); // 현재 있는 방 — 내 방이면 "돌아가기" 버튼을 숨긴다
  let hubOn = $state(false);
  async function toggleRoomMenu() {
    if (roomMenu !== null) { roomMenu = null; await expand(false); return; }
    const wasCollapsed = bubble === null;
    try {
      const [h, list, view] = await Promise.all([
        hubSettingsGet(),
        roomsList().catch(() => ({ rooms: [] })),
        roomView().catch(() => null),
      ]);
      hubOn = h.connected;
      myRoomId = h.room_id;
      // 위치 조회 실패 시 내 방으로 간주 — 돌아가기 버튼을 띄워봐야 이동도 실패한다
      curRoomId = view?.me.room_id ?? h.room_id;
      roomMenu = list.rooms;
    } catch {
      hubOn = false;
      roomMenu = [];
    }
    bubble = null; // 말풍선과 동시 표시 안 함
    if (wasCollapsed) await expand(true);
  }
  async function gotoRoom(roomId: string) {
    try { await roomGoto(roomId); } catch { /* cell_taken 등 — 다음 시도 */ }
    await closeRoomMenu();
  }
  async function closeRoomMenu() {
    if (roomMenu === null) return;
    roomMenu = null;
    await expand(false);
  }

  // 메뉴가 열려 있는 동안: 인원수 실시간 갱신(2s) + 포커스 잃으면 자동 닫힘
  $effect(() => {
    if (roomMenu === null) return;
    const t = setInterval(async () => {
      try { roomMenu = (await roomsList()).rooms; } catch { /* 서버 순단 — 다음 틱 */ }
    }, 2000);
    const onBlur = () => closeRoomMenu();
    window.addEventListener('blur', onBlur);
    return () => { clearInterval(t); window.removeEventListener('blur', onBlur); };
  });

  // 클릭 vs 드래그 (스펙 §6): drag-region 대신 수동 판별 — 클릭이면 홈피 열기
  let downAt: { x: number; y: number } | null = null;
  function onPointerDown(e: PointerEvent) {
    if (e.button === 2) return; // 우클릭은 contextmenu 핸들러가 처리
    // 캡처 없이는 빠른 드래그가 로봇 영역(128px)을 벗어난 뒤 move 이벤트가 끊겨
    // startDragging이 영영 호출되지 않는다 — 캡처로 창 밖까지 move를 계속 받는다
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
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

<div class="stage" class:expanded={bubble !== null || roomMenu !== null}>
  {#if roomMenu !== null}
    <div class="menu">
      <div class="menu-title">방 이동</div>
      {#if !hubOn}
        <button class="item" onclick={() => { openSettingsWindow(); closeRoomMenu(); }}>
          서버 미연결 — 설정 열기
        </button>
      {:else}
        {#if curRoomId !== myRoomId}
          <button class="item" onclick={() => gotoRoom(myRoomId)}>🏠 내 방으로 돌아가기</button>
        {/if}
        <div class="list">
          <!-- 지금 있는 방은 이동 대상이 아님 — 내 방은 위의 "돌아가기"가 담당 -->
          {#each roomMenu.filter((r) => r.room_id !== myRoomId && r.room_id !== curRoomId) as r (r.room_id)}
            <button class="item" onclick={() => gotoRoom(r.room_id)}>
              {r.owner_name}의 방 <span class="n">{r.occupants}</span>
            </button>
          {/each}
        </div>
      {/if}
    </div>
  {:else if bubble}
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
    oncontextmenu={(e) => { e.preventDefault(); toggleRoomMenu(); }}
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
  .menu {
    display: flex; flex-direction: column; gap: 4px;
    width: 200px; margin: 8px 12px 0 0; padding: 8px;
    background: var(--frame-bg); color: var(--ink);
    border-radius: var(--radius-m); box-shadow: var(--shadow-soft);
    font: 12px 'Segoe UI', 'Malgun Gothic', sans-serif;
  }
  .menu-title { font-weight: 700; font-size: 11px; color: var(--ink-soft); padding: 0 4px; }
  .menu .list { max-height: 96px; overflow-y: auto; display: flex; flex-direction: column; gap: 4px; }
  .menu .item {
    border: none; background: var(--pastel-lav); color: var(--ink);
    border-radius: var(--radius-s); padding: 6px 8px; font: inherit;
    cursor: pointer; text-align: left;
  }
  .menu .item:hover { background: var(--accent); color: #fff; }
  .menu .n { float: right; color: inherit; opacity: 0.7; }
</style>
