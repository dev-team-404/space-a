<script lang="ts">
  // 방 방문 뷰 (docs/design/room-visit.md §3)
  // 보이는 방 = 내 에이전트의 현재 방. 좌클릭 = 그 셀로 이동. 점유 셀은 커서로 차단 표시.
  import { robotSpecForSeed, roomMoveCell, roomView, type RoomMe, type RoomState } from '../api';
  import { drawRobot, type RobotSpec } from '../robot/render';
  import { frameAt } from '../robot/anim';

  let me = $state<RoomMe | null>(null);
  let room = $state<RoomState | null>(null);
  let error = $state('');
  let flash = $state(''); // cell_taken 등 일시 피드백

  // 파스텔 미니룸 팔레트 — 기존 MiniRoom의 그라데이션 벽 문법을 계승
  const WALL: Record<string, string> = {
    lavender: 'linear-gradient(180deg, #cfc6ec 0%, #e9e3f8 100%)',
    mint: 'linear-gradient(180deg, #bfe3d9 0%, #e3f4ee 100%)',
    cream: 'linear-gradient(180deg, #fdf1cf 0%, #fdf8e8 100%)',
    coral: 'linear-gradient(180deg, #f6cfc6 0%, #fbe8e3 100%)',
  };
  const FLOOR: Record<string, string> = {
    cream: '#fdf1cf', mint: '#bfe3d9', lavender: '#cfc6ec', wood: '#ead9c0',
  };
  // 이모지 가구 (window·rug는 CSS로 그림)
  const OBJECT_EMOJI: Record<string, string> = {
    plant: '🪴', sofa: '🛋️', lamp: '🪔', shelf: '📚', tv: '📺', cat: '🐈',
  };

  async function poll() {
    try {
      const v = await roomView();
      me = v.me;
      room = v.room;
      error = ''; // 방문 모드 전환은 App이 자체 폴링으로 감지
    } catch (e) {
      error = `${e}`;
    }
  }
  poll();
  $effect(() => {
    const t = setInterval(poll, 2000);
    // 숨김 창에서는 크로미움이 타이머를 절전시킴 — 창이 보이는 순간 즉시 갱신
    const onVis = () => { if (!document.hidden) poll(); };
    document.addEventListener('visibilitychange', onVis);
    window.addEventListener('focus', onVis);
    return () => {
      clearInterval(t);
      document.removeEventListener('visibilitychange', onVis);
      window.removeEventListener('focus', onVis);
    };
  });

  const occupied = $derived.by(() => {
    const s = new Set<string>();
    if (room) {
      for (const o of room.occupants) s.add(`${o.cell[0]},${o.cell[1]}`);
      for (const o of room.design.objects) s.add(`${o.cell[0]},${o.cell[1]}`);
    }
    return s;
  });

  async function clickCell(x: number, y: number) {
    if (occupied.has(`${x},${y}`)) return;
    try {
      await roomMoveCell(x, y);
      await poll();
    } catch (e) {
      // 동시 선점 패배(cell_taken) — 다음 폴에서 상태 정리됨
      flash = `${e}`.includes('cell_taken') ? '한 발 늦었어요 — 다른 에이전트가 선점' : `${e}`;
      setTimeout(() => (flash = ''), 2000);
    }
  }

  // 에이전트 로봇 렌더 — 서버가 공유한 마스코트 시드 사용 (없으면 이름 폴백):
  // 각자의 데스크톱 마스코트와 같은 로봇이 어느 클라이언트에서든 보인다
  const specCache = new Map<string, RobotSpec>();
  function robotCanvas(node: HTMLCanvasElement, name: string) {
    let raf = 0;
    const start = (spec: RobotSpec) => {
      const ctx = node.getContext('2d')!;
      const loop = (t: number) => {
        drawRobot(ctx, spec, frameAt('idle', t));
        raf = requestAnimationFrame(loop);
      };
      raf = requestAnimationFrame(loop);
    };
    const cached = specCache.get(name);
    if (cached) start(cached);
    else robotSpecForSeed(name).then((s) => { specCache.set(name, s); start(s); });
    return { destroy: () => cancelAnimationFrame(raf) };
  }
</script>

{#if room && me}
  <div class="roomwrap">
    <div class="title">
      <b>{room.owner_name}의 방</b>
      {#if me.my_room_id !== me.room_id}<span class="visiting">방문 중</span>{/if}
      <span class="count">{room.occupants.length}명</span>
      {#if flash}<span class="flash">{flash}</span>{/if}
    </div>
    <div class="room" style:background={WALL[room.design.wallpaper] ?? WALL.lavender}>
      <div class="floor" style:background={FLOOR[room.design.floor] ?? FLOOR.cream}></div>
      <div class="grid" style:--gw={room.grid.w} style:--gh={room.grid.h}>
        {#each Array(room.grid.h) as _, y}
          {#each Array(room.grid.w) as _, x}
            <button
              class="cell"
              class:taken={occupied.has(`${x},${y}`)}
              onclick={() => clickCell(x, y)}
              aria-label={`셀 ${x},${y}`}
            ></button>
          {/each}
        {/each}
        {#each room.design.objects as o (o.kind + o.cell.join())}
          {#if o.kind === 'window'}
            <div class="obj-window" style:--cx={o.cell[0]} style:--cy={o.cell[1]}></div>
          {:else if o.kind === 'rug'}
            <div class="obj-rug" style:--cx={o.cell[0]} style:--cy={o.cell[1]}></div>
          {:else}
            <div class="obj" style:--cx={o.cell[0]} style:--cy={o.cell[1]}>
              {OBJECT_EMOJI[o.kind] ?? '📦'}
            </div>
          {/if}
        {/each}
        {#each room.occupants as a (a.agent_id)}
          <div class="agent" class:mine={a.agent_id === me.agent_id} style:--cx={a.cell[0]} style:--cy={a.cell[1]}>
            <canvas width="128" height="128" use:robotCanvas={a.mascot_seed || a.name}></canvas>
            <span class="name">{a.name}{a.is_owner ? ' 🏠' : ''}</span>
          </div>
        {/each}
      </div>
    </div>
  </div>
{:else if error}
  <div class="empty">방 서버에 연결되지 않았어요 — 설정에서 Space A 서버를 연결하세요</div>
{/if}

<style>
  .roomwrap { display: flex; flex-direction: column; gap: 6px; }
  .title { display: flex; align-items: center; gap: 8px; font-size: 12px; color: var(--ink); }
  .visiting {
    background: var(--pastel-coral); border-radius: 999px; padding: 1px 8px; font-size: 11px;
  }
  .count { color: var(--ink-soft); font-size: 11px; }
  .flash { color: var(--accent); font-size: 11px; }
  .room {
    position: relative; border-radius: var(--radius-m); box-shadow: var(--shadow-soft);
    overflow: hidden; aspect-ratio: 30 / 16;
    /* 방이 페이지를 잡아먹지 않게 상한 — 폭은 비율 따라 자동 축소 */
    max-height: 300px; max-width: 563px; margin: 0 auto; width: 100%;
  }
  .floor {
    position: absolute; left: 0; right: 0; bottom: 0; height: 30%;
    box-shadow: inset 0 4px 6px -4px rgba(74, 70, 104, 0.3);
  }
  .grid {
    position: absolute; inset: 0;
    display: grid;
    grid-template-columns: repeat(var(--gw), 1fr);
    grid-template-rows: repeat(var(--gh), 1fr);
  }
  .cell {
    border: none; background: transparent; padding: 0; margin: 0;
    cursor: pointer;
  }
  .cell:hover { background: rgba(124, 111, 208, 0.14); }
  .cell.taken { cursor: not-allowed; }
  .cell.taken:hover { background: rgba(246, 207, 198, 0.4); }
  /* 셀 좌표 → 퍼센트 배치. 렌더는 셀보다 크게(폭 3셀분) 겹쳐 얹음 — 논리 점유는 1셀 */
  .agent, .obj {
    position: absolute; pointer-events: none;
    left: calc(var(--cx) / var(--gw) * 100%);
    top: calc(var(--cy) / var(--gh) * 100%);
    width: calc(100% / var(--gw));
    height: calc(100% / var(--gh));
    display: flex; align-items: flex-end; justify-content: center;
  }
  .agent canvas {
    width: 300%; height: auto; aspect-ratio: 1;
    image-rendering: pixelated;
    transform: translateY(8%);
  }
  /* 로봇 캔버스는 셀 위로 크게 겹쳐 그려지므로 이름표는 발밑(셀 아래)에 — 캐릭터를 가리지 않게.
     캔버스가 translateY(8%)로 셀 아래로 살짝 내려오므로 발끝과 붙지 않게 여유를 둔다 */
  .agent .name {
    position: absolute; top: calc(100% + 8px); left: 50%; transform: translateX(-50%);
    font-size: 10px; color: var(--ink); background: var(--frame-bg);
    border-radius: 999px; padding: 0 6px; white-space: nowrap;
    box-shadow: var(--shadow-soft);
  }
  .agent.mine .name { background: var(--accent); color: #fff; }
  .obj { font-size: 22px; filter: drop-shadow(0 2px 2px rgba(74, 70, 104, 0.25)); }
  /* 창문 — MiniRoom 문법: 민트 유리 + 크림 창틀. 논리 점유 1셀, 렌더 4×3셀 */
  .obj-window {
    position: absolute; pointer-events: none;
    left: calc(var(--cx) / var(--gw) * 100%);
    top: calc(var(--cy) / var(--gh) * 100%);
    width: calc(100% / var(--gw) * 4);
    height: calc(100% / var(--gh) * 3);
    background: var(--pastel-mint);
    border-radius: var(--radius-s);
    box-shadow: inset 0 0 0 4px #fffdfa, inset 0 0 0 5px rgba(74, 70, 104, 0.12);
  }
  /* 러그 — 타원, 렌더 6×2셀 (중심 셀 기준) */
  .obj-rug {
    position: absolute; pointer-events: none;
    left: calc((var(--cx) - 2.5) / var(--gw) * 100%);
    top: calc(var(--cy) / var(--gh) * 100%);
    width: calc(100% / var(--gw) * 6);
    height: calc(100% / var(--gh) * 2);
    border-radius: 50%;
    background: rgba(255, 253, 250, 0.55);
    box-shadow: inset 0 0 0 3px rgba(74, 70, 104, 0.08);
  }
  .empty {
    font-size: 12px; color: var(--ink-soft); background: var(--frame-bg);
    border-radius: var(--radius-m); box-shadow: var(--shadow-soft); padding: 14px;
  }
</style>
