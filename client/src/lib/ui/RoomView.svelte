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

  const WALL: Record<string, string> = {
    lavender: 'var(--pastel-lav)', mint: 'var(--pastel-mint)',
    cream: 'var(--pastel-cream)', coral: 'var(--pastel-coral)',
  };
  const FLOOR: Record<string, string> = {
    cream: 'var(--pastel-cream)', mint: 'var(--pastel-mint)', lavender: 'var(--pastel-lav)',
  };
  const OBJECT_EMOJI: Record<string, string> = { plant: '🪴', rug: '🟢', lamp: '🛋️', window: '🪟' };

  async function poll() {
    try {
      const v = await roomView();
      me = v.me;
      room = v.room;
      error = '';
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

  // 에이전트 로봇 렌더 — 시드=이름이라 어느 클라이언트에서 봐도 같은 모습
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
          <div class="obj" style:--cx={o.cell[0]} style:--cy={o.cell[1]}>
            {OBJECT_EMOJI[o.kind] ?? '📦'}
          </div>
        {/each}
        {#each room.occupants as a (a.agent_id)}
          <div class="agent" class:mine={a.agent_id === me.agent_id} style:--cx={a.cell[0]} style:--cy={a.cell[1]}>
            <canvas width="128" height="128" use:robotCanvas={a.name}></canvas>
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
  }
  .floor { position: absolute; left: 0; right: 0; bottom: 0; height: 30%; }
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
  .agent .name {
    position: absolute; top: -14px; left: 50%; transform: translateX(-50%);
    font-size: 10px; color: var(--ink); background: var(--frame-bg);
    border-radius: 999px; padding: 0 6px; white-space: nowrap;
    box-shadow: var(--shadow-soft);
  }
  .agent.mine .name { background: var(--accent); color: #fff; }
  .obj { font-size: 18px; }
  .empty {
    font-size: 12px; color: var(--ink-soft); background: var(--frame-bg);
    border-radius: var(--radius-m); box-shadow: var(--shadow-soft); padding: 14px;
  }
</style>
