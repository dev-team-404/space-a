// 방 씬 렌더러 — 완성된 방 배경 프리셋(room.rN, 통짜 이미지) 위에 책상·로봇만 얹는다.
// 배경 이미지는 manifest cal(좌우 바닥 꼭짓점 + slope)로 2:1 격자 좌표계에 정규화된다.
// 책상은 킷 스프라이트(desk.dN)가 있으면 Sprite, 없으면 Graphics 아이소 박스로 폴백.

import { Container, Graphics, Sprite, Text, TextStyle } from 'pixi.js'
import type { SpaceAgent, SpaceIssue } from '../api'
import { DESKS, resolveDeskId, resolveRoomId } from './catalog'
import { TILE_W, TILE_H, isoX, isoY, depth } from './iso'
import { kitPiece, kitPieceScale } from './kit'
import type { RoomConfig } from './types'

export type RoomSceneCallbacks = {
  onAgentTap?: (agent: SpaceAgent) => void
  /** 배경 칠판 영역 클릭 → 이슈 패널 */
  onBoardTap?: () => void
  /** 배경 책장 영역 클릭 → 지식 패널 */
  onShelfTap?: () => void
}

export type RoomSceneData = {
  agents: SpaceAgent[]
  issues: SpaceIssue[]
  knowledgeCount: number
}

/** 빌더 프리뷰용 샘플 — 실데이터 fetch 없이 방 모양만 확인 */
export const PREVIEW_DATA: RoomSceneData = {
  agents: [
    { agent_id: 'p1', name: 'Agent_A', role: '', owner: '', status: 'working', status_line: '', last_active_at: null },
    { agent_id: 'p2', name: 'Agent_B', role: '', owner: '', status: 'idle', status_line: '', last_active_at: null },
  ],
  issues: [
    { issue_id: 'i1', title: '배포 후 5xx 급증', status: 'open', opened_by: '', timeline: [] },
    { issue_id: 'i2', title: '인증서 문제', status: 'resolved', opened_by: '', timeline: [] },
  ],
  knowledgeCount: 12,
}

const ROBOT_COLORS = [0xd9a441, 0x7fb3d5, 0xa3be8c, 0xd08770, 0xb48ead, 0x8fbcbb]

function pt(gx: number, gy: number, dy = 0): [number, number] {
  return [isoX(gx, gy), isoY(gx, gy) + dy]
}

/** 아이소 박스: 윗면 + 앞쪽 두 옆면. h는 px 높이. */
function drawIsoBox(
  g: Graphics,
  gx: number,
  gy: number,
  w: number,
  d: number,
  h: number,
  colors: { top: number; left: number; right: number },
) {
  const [ax, ay] = pt(gx, gy, -h)
  const [bx, by] = pt(gx + w, gy, -h)
  const [cx, cy] = pt(gx + w, gy + d, -h)
  const [dx, dyv] = pt(gx, gy + d, -h)
  // 왼쪽 옆면 (남서향)
  g.poly([dx, dyv, cx, cy, cx, cy + h, dx, dyv + h]).fill(colors.left)
  // 오른쪽 옆면 (남동향)
  g.poly([bx, by, cx, cy, cx, cy + h, bx, by + h]).fill(colors.right)
  // 윗면
  g.poly([ax, ay, bx, by, cx, cy, dx, dyv]).fill(colors.top)
}

/**
 * 킷 스프라이트 배치 — 있으면 Sprite를 얹고 true, 없으면 false(호출부가 Graphics 폴백).
 * floor 부품 앵커: footprint 다이아몬드의 남쪽(앞) 꼭짓점 = 이미지 하단 중앙.
 */
function placeKitSprite(root: Container, key: string, gx: number, gy: number, zIndex: number, mul = 1): Sprite | null {
  const piece = kitPiece(key)
  if (!piece || piece.mount !== 'floor') return null
  const [w, d] = piece.footprint
  const sp = new Sprite(piece.texture)
  sp.anchor.set(0.5, 1)
  const s = kitPieceScale() * mul
  sp.scale.set(s)
  const [x, y] = pt(gx + w, gy + d)
  sp.position.set(x + piece.offset[0] * s, y + piece.offset[1] * s)
  sp.zIndex = zIndex
  root.addChild(sp)
  return sp
}

function shade(color: number, f: number): number {
  const r = Math.min(255, Math.round(((color >> 16) & 0xff) * f))
  const g = Math.min(255, Math.round(((color >> 8) & 0xff) * f))
  const b = Math.min(255, Math.round((color & 0xff) * f))
  return (r << 16) | (g << 8) | b
}

/** 책상 배치 계산 — 에이전트 수에 맞춰 격자를 자동으로 늘린다 (책상 5개 고정 한계 해소). */
function layoutDesks(count: number): { slots: { gx: number; gy: number }[]; W: number; H: number } {
  const cols = Math.min(3, Math.max(1, count))
  const rows = Math.max(1, Math.ceil(count / 3))
  const slots: { gx: number; gy: number }[] = []
  for (let k = 0; k < count; k++) {
    const col = k % 3
    const row = Math.floor(k / 3)
    slots.push({ gx: 1.5 + col * 3, gy: 2.5 + row * 3 })
  }
  const W = Math.max(8, 1.5 + cols * 3 + 0.5)
  const H = Math.max(8, 2.5 + rows * 3 + 1.5)
  return { slots, W, H }
}

export function buildRoomScene(
  config: RoomConfig,
  data: RoomSceneData,
  cb: RoomSceneCallbacks = {},
): Container {
  const root = new Container()
  root.sortableChildren = true

  const desk = DESKS[resolveDeskId(config.desk)]

  // 책상 수 — 'auto'(또는 구버전 저장분)면 에이전트 수를 따라감
  const deskCount = Math.max(
    1,
    config.desks == null || config.desks === 'auto' ? data.agents.length : config.desks,
  )
  const { slots: baseSlots, W: LW, H: LH } = layoutDesks(deskCount)
  // 방 배경(정방형 다이아)에 맞춰 방은 정사각 격자. 기본 16칸, 책상이 넘치면 확장.
  const W = Math.max(16, LW, LH)
  const H = W

  // ── 방 배경 프리셋 (room.rN — 벽·바닥·책장·칠판이 다 그려진 통짜 이미지 한 장) ──
  // 캘리브레이션(manifest cal)으로 이미지를 2:1 격자 좌표계에 정규화한다:
  // 좌우 바닥 꼭짓점 ↔ 격자 (0,W)/(W,0), 이미지 기울기(slope)는 비등방 스케일로 보정.
  const roomId = resolveRoomId(config.room)
  const roomKit = kitPiece(`room.${roomId}`) ?? kitPiece('shell.room')
  const roomCal = roomKit?.cal
  let cut = 0 // 뒷벽이 잘라먹는 격자 깊이: gx+gy < cut 영역은 벽 뒤
  if (roomKit && roomCal) {
    const [lxI, lyI] = roomCal.left
    const [rxI] = roomCal.right
    const cxI = (lxI + rxI) / 2
    const ax = (rxI - lxI) / (2 * W) // 이미지 px / 격자 x단위
    const ay = ax * (roomCal.slope ?? 0.5)
    const cyI = lyI - W * ay // 가상 다이아 원점의 이미지 y
    const sx = TILE_W / 2 / ax
    const sy = TILE_H / 2 / ay
    const sp = new Sprite(roomKit.texture)
    sp.scale.set(sx, sy)
    sp.position.set(-cxI * sx, -cyI * sy)
    sp.zIndex = -1000
    root.addChild(sp)
    cut = (roomCal.backEdgeY - cyI) / ay

    // 배경에 이미 그려진 칠판·책장 영역에 투명 히트존 → 이슈/지식 패널 진입점 유지.
    // cal 좌표(이미지 px)를 배경 스프라이트와 같은 변환(sx/sy, -cxI/-cyI)으로 화면에 맞춘다.
    const hitZone = (
      area: [number, number, number, number],
      z: number,
      onTap?: () => void,
    ) => {
      if (!onTap) return
      const [x0, y0, x1, y1] = area
      const hz = new Graphics()
        .rect((x0 - cxI) * sx, (y0 - cyI) * sy, (x1 - x0) * sx, (y1 - y0) * sy)
        .fill({ color: 0xffffff, alpha: 0.001 }) // 투명하지만 히트 판정은 살아 있게
      hz.zIndex = z
      hz.eventMode = 'static'
      hz.cursor = 'pointer'
      hz.on('pointertap', onTap)
      root.addChild(hz)
    }
    hitZone(roomCal.backWall, -800, cb.onBoardTap)
    if (roomCal.shelfArea) hitZone(roomCal.shelfArea, -800, cb.onShelfTap)
  }

  // 책상 클러스터는 방 중앙에 — 단, 뒷벽 컷 안쪽으로 (gx+gy ≥ cut)
  const dx = (W - LW) / 2
  const dy = Math.max(1.2, (H - LH) / 2, cut + 2 - 4 - (W - LW) / 2)
  const slots = baseSlots.map((s) => ({ gx: s.gx + dx, gy: s.gy + dy }))

  // ── 책상 (deskCount만큼 — 에이전트보다 많으면 빈 책상, 'mix'면 종류 순환) ──
  // 새 desk 스프라이트가 시트보다 작게 잘려 나와, 배경 대비 살짝 키워 얹는다.
  const DESK_SCALE = 1.7
  slots.forEach(({ gx, gy }, k) => {
    const deskId = resolveDeskId(config.desk, k)
    if (placeKitSprite(root, `desk.${deskId}`, gx, gy, depth(gx + 1, gy + 0.5), DESK_SCALE)) return
    const dp = DESKS[deskId]
    const dg = new Graphics()
    drawIsoBox(dg, gx, gy, 2, 1, 34, { top: dp.top, left: shade(dp.side, 0.9), right: dp.side })
    // 모니터
    const [mx, my] = pt(gx + 1.0, gy + 0.35, -34)
    dg.poly([mx - 14, my - 30, mx + 14, my - 16, mx + 14, my + 4, mx - 14, my - 10]).fill(0x23262b)
    dg.poly([mx - 11, my - 26, mx + 11, my - 15, mx + 11, my - 1, mx - 11, my - 12]).fill(0x3a4550)
    dg.zIndex = depth(gx + 1, gy + 0.5)
    root.addChild(dg)
  })

  // ── 로봇 — 책상에 배치하고, 자리가 모자라면 방 앞쪽에 서 있음 ──
  const nameStyle = new TextStyle({ fill: 0xe8eaed, fontSize: 11 })
  const makeRobot = (agent: SpaceAgent, k: number, rgx: number, rgy: number) => {
    const robot = new Container()
    const [rx, ry] = pt(rgx, rgy)
    robot.position.set(rx, ry)
    robot.zIndex = depth(rgx, rgy)
    const body = ROBOT_COLORS[k % ROBOT_COLORS.length]
    const rg = new Graphics()
    rg.ellipse(0, 2, 16, 7).fill({ color: 0x000000, alpha: 0.25 }) // 그림자
    rg.roundRect(-13, -34, 26, 30, 9).fill(body) // 몸통
    rg.roundRect(-10, -30, 20, 10, 5).fill(0x23262b) // 바이저
    rg.circle(-4, -25, 2.2).fill(0x9fe8ff) // 눈
    rg.circle(4, -25, 2.2).fill(0x9fe8ff)
    rg.moveTo(0, -34).lineTo(0, -41).stroke({ color: shade(body, 0.7), width: 2 }) // 안테나
    rg.circle(0, -43, 3).fill(agent.status === 'working' ? 0x9fe86a : 0x767d87)
    robot.addChild(rg)
    const nameTag = new Text({ text: agent.name, style: nameStyle })
    nameTag.position.set(-nameTag.width / 2, 6)
    // 앞줄 책상 스프라이트 위에서도 읽히도록 반투명 필 배경
    const namePill = new Graphics()
      .roundRect(-nameTag.width / 2 - 5, 4, nameTag.width + 10, 17, 8)
      .fill({ color: 0x0d1220, alpha: 0.7 })
    robot.addChild(namePill, nameTag)
    // 작업 중이면 말풍선 점 표시
    if (agent.status === 'working') {
      const bub = new Graphics()
      bub.roundRect(12, -52, 30, 16, 8).fill(0xf0e6d2)
      bub.poly([16, -37, 24, -37, 15, -30]).fill(0xf0e6d2)
      bub.circle(20, -44, 1.8).fill(0x555)
      bub.circle(27, -44, 1.8).fill(0x555)
      bub.circle(34, -44, 1.8).fill(0x555)
      robot.addChild(bub)
    }
    robot.eventMode = 'static'
    robot.cursor = 'pointer'
    robot.on('pointertap', () => cb.onAgentTap?.(agent))
    root.addChild(robot)
  }
  data.agents.forEach((agent, k) => {
    const slot = slots[k]
    if (slot) makeRobot(agent, k, slot.gx + 1, slot.gy + 1.6) // 책상 앞에 앉음
    else makeRobot(agent, k, dx + 1.5 + ((k - slots.length) % 5) * 1.7, dy + LH - 1.2) // 서 있음
  })

  return root
}
