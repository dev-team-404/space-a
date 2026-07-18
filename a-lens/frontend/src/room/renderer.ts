// 방 씬 렌더러 — RoomConfig(격자 + variant 데이터)만으로 방을 그린다.
// 이미지 캘리브레이션 없음: 가구는 전부 Graphics 프리미티브 아이소 박스.
// 스프라이트 에셋이 생기면 drawIsoBox 자리가 Sprite로 바뀌고 좌표계는 그대로 유지된다.

import { Container, Graphics, Sprite, Text, TextStyle } from 'pixi.js'
import type { SpaceAgent, SpaceIssue } from '../api'
import {
  BOARDS,
  DESKS,
  FLOORS,
  SHELF_SIDE,
  WALLPAPERS,
  resolveDeskId,
  resolveFloorId,
  resolveShelfId,
  resolveWallpaperId,
} from './catalog'
import { TILE_W, TILE_H, WALL_H, isoX, isoY, depth } from './iso'
import { kitPiece, kitPieceScale } from './kit'
import type { RoomConfig } from './types'

export type RoomSceneCallbacks = {
  onAgentTap?: (agent: SpaceAgent) => void
  onBoardTap?: () => void
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
const ISSUE_DOT: Record<string, number> = {
  open: 0xd08770,
  knowledge_linked: 0x7fb3d5,
  resolved: 0xa3be8c,
}

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
function placeKitSprite(root: Container, key: string, gx: number, gy: number, zIndex: number): Sprite | null {
  const piece = kitPiece(key)
  if (!piece || piece.mount !== 'floor') return null
  const [w, d] = piece.footprint
  const sp = new Sprite(piece.texture)
  sp.anchor.set(0.5, 1)
  const s = kitPieceScale()
  sp.scale.set(s)
  const [x, y] = pt(gx + w, gy + d)
  sp.position.set(x + piece.offset[0] * s, y + piece.offset[1] * s)
  sp.zIndex = zIndex
  root.addChild(sp)
  return sp
}

/** wall-right 부품(칠판 등) — 오른쪽 뒷벽(gy=0 면) 원근으로 미리 그려진 에셋. 앵커: 이미지 좌측 하단. */
function placeWallSprite(root: Container, key: string, gxStart: number, bottomPx: number, zIndex: number): Sprite | null {
  const piece = kitPiece(key)
  if (!piece || piece.mount !== 'wall-right') return null
  const sp = new Sprite(piece.texture)
  sp.anchor.set(0, 1)
  const s = kitPieceScale()
  sp.scale.set(s)
  const [x, y] = pt(gxStart, 0, -bottomPx)
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

  const wallId = resolveWallpaperId(config.wallpaper)
  const floorId = resolveFloorId(config.floor)
  const wall = WALLPAPERS[wallId]
  const floor = FLOORS[floorId]
  const desk = DESKS[resolveDeskId(config.desk)]
  const board = BOARDS[config.board]

  // 책상 수 — 'auto'(또는 구버전 저장분)면 에이전트 수를 따라감
  const deskCount = Math.max(
    1,
    config.desks == null || config.desks === 'auto' ? data.agents.length : config.desks,
  )
  const { slots: baseSlots, W: LW, H: LH } = layoutDesks(deskCount)
  // 셸 스프라이트(정방형 다이아)에 맞춰 방은 정사각 격자. 기본 16칸, 책상이 넘치면 확장.
  const W = Math.max(config.size ?? 16, LW, LH)
  const H = W

  // ── 고정 방 셸 (shell.room, 3면 벽 팔각 방 한 장) — 있으면 floor./wall. 부품 대신 이걸 쓴다 ──
  // 캘리브레이션(manifest cal)으로 이미지를 2:1 격자 좌표계에 정규화한다:
  // 좌우 바닥 꼭짓점 ↔ 격자 (0,W)/(W,0), 이미지 기울기(slope)는 비등방 스케일로 보정.
  const shellKit = kitPiece('shell.room')
  const shellCal = shellKit?.cal
  let wallH = WALL_H
  let cut = 0 // 뒷벽(평면)이 잘라먹는 격자 깊이: gx+gy < cut 영역은 벽 뒤
  let flatBoard: { x0: number; y0: number; x1: number; y1: number } | null = null
  if (shellKit && shellCal) {
    const [lxI, lyI] = shellCal.left
    const [rxI] = shellCal.right
    const cxI = (lxI + rxI) / 2
    const ax = (rxI - lxI) / (2 * W) // 이미지 px / 격자 x단위
    const ay = ax * (shellCal.slope ?? 0.5)
    const cyI = lyI - W * ay // 가상 다이아 원점의 이미지 y
    const sx = TILE_W / 2 / ax
    const sy = TILE_H / 2 / ay
    const sp = new Sprite(shellKit.texture)
    sp.scale.set(sx, sy)
    sp.position.set(-cxI * sx, -cyI * sy)
    sp.zIndex = -1000
    root.addChild(sp)
    cut = (shellCal.backEdgeY - cyI) / ay
    const bwCal = shellCal.backWall
    flatBoard = {
      x0: (bwCal[0] - cxI) * sx,
      y0: (bwCal[1] - cyI) * sy,
      x1: (bwCal[2] - cxI) * sx,
      y1: (bwCal[3] - cyI) * sy,
    }
    wallH = flatBoard.y1 - flatBoard.y0
  }

  // 책상 클러스터는 방 중앙에 — 단, 뒷벽 컷 안쪽으로 (gx+gy ≥ cut)
  const dx = (W - LW) / 2
  const dy = Math.max(1.2, (H - LH) / 2, cut + 2 - 4 - (W - LW) / 2)
  const slots = baseSlots.map((s) => ({ gx: s.gx + dx, gy: s.gy + dy }))

  // ── 바닥 (셸 스프라이트: 윗꼭짓점을 격자 원점에 정렬, 격자 폭에 맞춰 스케일) ──
  const floorKit = shellKit ? null : kitPiece(`floor.${floorId}`)
  if (floorKit) {
    const s = (W * TILE_W) / floorKit.texture.width
    const sp = new Sprite(floorKit.texture)
    sp.anchor.set(floorKit.anchor[0], floorKit.anchor[1])
    sp.scale.set(s)
    const [ox, oy] = pt(0, 0)
    sp.position.set(ox + floorKit.offset[0] * s, oy + floorKit.offset[1] * s)
    sp.zIndex = -1000
    root.addChild(sp)
  } else if (!shellKit) {
    const floorG = new Graphics()
    floorG.zIndex = -1000
    for (let gx = 0; gx < W; gx++) {
      for (let gy = 0; gy < H; gy++) {
        const alt = (gx + gy) % 2 === 0
        const [tx, ty] = pt(gx, gy)
        floorG
          .poly([tx, ty, tx + TILE_W / 2, ty + TILE_H / 2, tx, ty + TILE_H, tx - TILE_W / 2, ty + TILE_H / 2])
          .fill(alt ? floor.a : floor.b)
          .stroke({ color: floor.edge, width: 1, alpha: 0.35 })
      }
    }
    floorG
      .poly([...pt(0, 0), ...pt(W, 0), ...pt(W, H), ...pt(0, H)])
      .stroke({ color: floor.edge, width: 3, alpha: 0.9 })
    root.addChild(floorG)
  }

  // ── 뒷벽 (셸 스프라이트: V자 안쪽 코너를 격자 원점에 정렬) ──
  // 칠판·조명·보드 텍스트는 실제 벽면 높이(wallH)를 기준으로 배치된다.
  const wallKit = shellKit ? null : kitPiece(`wall.${wallId}`)
  if (wallKit) {
    const s = (W * TILE_W) / wallKit.texture.width
    wallH = wallKit.faceH * s
    const sp = new Sprite(wallKit.texture)
    sp.anchor.set(wallKit.anchor[0], wallKit.anchor[1])
    sp.scale.set(s)
    const [ox, oy] = pt(0, 0)
    sp.position.set(ox + wallKit.offset[0] * s, oy + wallKit.offset[1] * s)
    sp.zIndex = -900
    root.addChild(sp)
  } else if (!shellKit) {
    const wallsG = new Graphics()
    wallsG.zIndex = -900
    // 오른쪽 뒷벽 (gy=0 면)
    wallsG.poly([...pt(0, 0, -wallH), ...pt(W, 0, -wallH), ...pt(W, 0), ...pt(0, 0)]).fill(wall.wall)
    // 왼쪽 뒷벽 (gx=0 면)
    wallsG.poly([...pt(0, 0, -wallH), ...pt(0, H, -wallH), ...pt(0, H), ...pt(0, 0)]).fill(wall.shade)
    // 걸레받이 트림
    wallsG.poly([...pt(0, 0, -10), ...pt(W, 0, -10), ...pt(W, 0), ...pt(0, 0)]).fill(wall.trim)
    wallsG.poly([...pt(0, 0, -10), ...pt(0, H, -10), ...pt(0, H), ...pt(0, 0)]).fill(shade(wall.trim, 0.85))
    root.addChild(wallsG)
  }

  // ── 칠판 — 클릭 → 이슈 패널 ──
  const chalkSize = Math.max(13, Math.round(wallH * 0.055))
  const chalkGap = chalkSize * 1.9
  const chalkStyle = new TextStyle({ fill: board.chalk, fontSize: chalkSize })
  const issueLine = (issue: SpaceIssue): [Graphics, Text] => {
    const dot = new Graphics().circle(chalkSize * 0.45, chalkSize * 0.55, chalkSize * 0.38).fill(ISSUE_DOT[issue.status] ?? 0xd9a441)
    const label = new Text({
      text: issue.title.length > 16 ? issue.title.slice(0, 16) + '…' : issue.title,
      style: chalkStyle,
    })
    label.x = chalkSize * 1.2
    return [dot, label]
  }

  if (flatBoard) {
    // 고정 셸: 평면 뒷벽 정면에 왜곡 없는 칠판
    const fbW = flatBoard.x1 - flatBoard.x0
    const fbH = flatBoard.y1 - flatBoard.y0
    const rx0 = flatBoard.x0 + fbW * 0.17
    const rw = fbW * 0.66
    const ry0 = flatBoard.y0 + fbH * 0.08
    const rh = fbH * 0.8
    const boardG = new Graphics()
    boardG.zIndex = -890
    boardG.roundRect(rx0, ry0, rw, rh, 4).fill(board.face).stroke({ color: board.frame, width: 6 })
    boardG.eventMode = 'static'
    boardG.cursor = 'pointer'
    boardG.on('pointertap', () => cb.onBoardTap?.())
    root.addChild(boardG)

    const lines = data.issues.slice(0, 3)
    lines.forEach((issue, i) => {
      const line = new Container()
      line.position.set(rx0 + chalkSize, ry0 + chalkSize * 0.8 + i * chalkGap)
      line.zIndex = -889
      line.addChild(...issueLine(issue))
      root.addChild(line)
    })
    if (lines.length === 0) {
      const empty = new Text({ text: '이슈 없음', style: chalkStyle })
      empty.position.set(rx0 + chalkSize, ry0 + chalkSize)
      empty.zIndex = -889
      root.addChild(empty)
    }
  } else {
    // 코너 셸/폴백: 오른쪽 뒷벽(gy=0 면)에 벽 기울기로 부착
    const bA = Math.max(1.0, W * 0.1) // 벽을 따라 시작 cell
    const bB = Math.min(W - 1, W * 0.55)
    const bBot = wallH * 0.22
    const bTop = Math.min(wallH * 0.82, bBot + 260)
    const boardSprite = placeWallSprite(root, `board.${config.board}`, bA, bBot, -890)
    const boardHit: Container = boardSprite ?? new Graphics()
    if (!boardSprite) {
      const boardG = boardHit as Graphics
      boardG.zIndex = -890
      boardG
        .poly([...pt(bA, 0, -bTop), ...pt(bB, 0, -bTop), ...pt(bB, 0, -bBot), ...pt(bA, 0, -bBot)])
        .fill(board.face)
        .stroke({ color: board.frame, width: 5 })
      root.addChild(boardG)
    }
    boardHit.eventMode = 'static'
    boardHit.cursor = 'pointer'
    boardHit.on('pointertap', () => cb.onBoardTap?.())

    const wallSkew = Math.atan2(TILE_H / 2, TILE_W / 2)
    data.issues.slice(0, 3).forEach((issue, i) => {
      const line = new Container()
      const [lx, ly] = pt(bA + 0.35, 0, -(bTop - chalkSize * 1.7 - i * chalkGap))
      line.position.set(lx, ly)
      line.skew.y = wallSkew
      line.zIndex = -889
      line.addChild(...issueLine(issue))
      root.addChild(line)
    })
    if (data.issues.length === 0) {
      const empty = new Text({ text: '이슈 없음', style: chalkStyle })
      const [ex, ey] = pt(bA + 0.35, 0, -(bTop - chalkSize * 2))
      empty.position.set(ex, ey)
      empty.skew.y = wallSkew
      empty.zIndex = -889
      root.addChild(empty)
    }
  }

  // ── 책장 — variant에 따라 왼쪽/오른쪽 벽에 배치. 클릭 → 지식 패널 ──
  const shelfId = resolveShelfId(config.shelf)
  const shelfKit = kitPiece(`shelf.${shelfId}`)
  let shelfSprite: Sprite | null = null
  let badgePos: [number, number] = [0, 0]
  if (shelfKit) {
    const [fw, fd] = shelfKit.footprint
    const pos =
      SHELF_SIDE[shelfId] === 'left'
        ? { gx: 0.15, gy: Math.max(1.2, cut + 0.4) } // 왼쪽 벽 앞 (뒷벽 컷 안쪽)
        : { gx: W - fw - 0.4, gy: 0.18 } // 오른쪽 벽 앞
    shelfSprite = placeKitSprite(root, `shelf.${shelfId}`, pos.gx, pos.gy, depth(pos.gx + fw, pos.gy + fd))
    if (shelfSprite) badgePos = [shelfSprite.x, shelfSprite.y - shelfSprite.height - 8]
  }
  const shelfHit: Container = shelfSprite ?? new Graphics()
  if (!shelfSprite) {
    const shelfG = shelfHit as Graphics
    const shelfDepth = 0.7
    drawIsoBox(shelfG, 0.15, 1.2, shelfDepth, 2.4, 105, {
      top: shade(desk.top, 0.9),
      left: shade(desk.side, 0.95),
      right: desk.side,
    })
    // 책 등 — 오른쪽(남동향) 면에 줄무늬
    const bookColors = [0xa3be8c, 0xd08770, 0x7fb3d5, 0xd9a441, 0xb48ead]
    for (let row = 0; row < 3; row++) {
      for (let i = 0; i < 5; i++) {
        const t = 0.35 + row * 0.62
        const [sx, sy] = pt(0.15 + shelfDepth, 1.35 + i * 0.42, -(105 - 14 - row * 30))
        shelfG.poly([sx, sy, sx + 7, sy + 3.5, sx + 7, sy + 21.5, sx, sy + 18]).fill(bookColors[(i + row + Math.floor(t)) % 5])
      }
    }
    shelfG.zIndex = depth(1, 2)
    root.addChild(shelfG)
    const [fx, fy] = pt(0.5, 2.2, -118)
    badgePos = [fx, fy]
  }
  shelfHit.eventMode = 'static'
  shelfHit.cursor = 'pointer'
  shelfHit.on('pointertap', () => cb.onShelfTap?.())

  const countStyle = new TextStyle({
    fill: 0xf0e6d2,
    fontSize: Math.max(12, Math.round(wallH * 0.05)),
    fontWeight: 'bold',
  })
  const shelfBadge = new Text({ text: `지식 ${data.knowledgeCount}`, style: countStyle })
  shelfBadge.position.set(badgePos[0] - shelfBadge.width / 2, badgePos[1])
  shelfBadge.zIndex = (shelfSprite?.zIndex ?? depth(1, 2)) + 0.1
  root.addChild(shelfBadge)

  // ── 장식 (킷 스프라이트 우선, 없으면 Graphics 폴백) ──
  if (config.deco.includes('rug')) {
    const rc = { gx: W * 0.62 - 1.4, gy: H * 0.62 - 1.4 }
    if (!placeKitSprite(root, 'deco.rug', rc.gx, rc.gy, -800)) {
      const rug = new Graphics()
      rug
        .poly([...pt(rc.gx, rc.gy), ...pt(rc.gx + 2.8, rc.gy), ...pt(rc.gx + 2.8, rc.gy + 2.8), ...pt(rc.gx, rc.gy + 2.8)])
        .fill({ color: 0xb0524a, alpha: 0.9 })
        .stroke({ color: 0x8a3e38, width: 3 })
      rug.zIndex = -800
      root.addChild(rug)
    }
  }
  const shelfOnRight = SHELF_SIDE[shelfId] === 'right'
  if (config.deco.includes('plant')) {
    const plantAt = (gx: number, gy: number) => {
      if (placeKitSprite(root, 'deco.plant', gx, gy, depth(gx, gy))) return
      const p = new Graphics()
      drawIsoBox(p, gx, gy, 0.55, 0.55, 16, { top: 0x8a5b2e, left: 0x6b4423, right: 0x7a4e28 })
      const [px, py] = pt(gx + 0.28, gy + 0.28, -16)
      p.circle(px, py - 16, 13).fill(0x4e7a3a)
      p.circle(px - 10, py - 8, 9).fill(0x5d8c46)
      p.circle(px + 10, py - 9, 10).fill(0x446b33)
      p.zIndex = depth(gx, gy)
      root.addChild(p)
    }
    // 책장이 오른벽이면 화분은 비어 있는 왼벽 쪽으로 (뒷벽 컷 안쪽)
    if (shelfOnRight) plantAt(0.7, Math.max(2.6, cut + 0.6))
    else plantAt(W - 1.4, 0.6)
    plantAt(0.6, H - 1.6)
  }
  if (config.deco.includes('water-cooler')) {
    const wgx = shelfOnRight ? 0.5 : W - 1.3
    const wgy = shelfOnRight ? Math.max(4.0, cut + 0.8) : 1.0
    if (!placeKitSprite(root, 'deco.water-cooler', wgx, wgy, depth(wgx, wgy))) {
      const wc = new Graphics()
      drawIsoBox(wc, wgx, wgy, 0.6, 0.6, 46, { top: 0xc9d2d8, left: 0xa9b4bc, right: 0xb8c4cc })
      const [wx, wy] = pt(wgx + 0.3, wgy + 0.3, -46)
      wc.ellipse(wx, wy - 10, 10, 12).fill({ color: 0x5b9bd5, alpha: 0.9 })
      wc.zIndex = depth(wgx, wgy)
      root.addChild(wc)
    }
  }
  if (config.deco.includes('string-lights')) {
    const lights = new Graphics()
    lights.zIndex = -880
    if (flatBoard) {
      // 고정 셸: 평면 뒷벽 상단을 따라 전구 줄
      const span = flatBoard.x1 - flatBoard.x0
      const n = Math.max(6, Math.floor(span / 34))
      for (let i = 0; i <= n; i++) {
        const lx = flatBoard.x0 + (span * i) / n
        const ly = flatBoard.y0 + 4 + (i % 2) * 5
        lights.circle(lx, ly, 7).fill({ color: 0xe8b54a, alpha: 0.25 })
        lights.circle(lx, ly, 3.5).fill(0xf5d78e)
      }
    } else {
      const n = Math.floor(W)
      for (let i = 0; i < n; i++) {
        const [lx, ly] = pt(i + 0.5, 0, -(wallH - 12 - (i % 2) * 6))
        lights.circle(lx, ly, 7).fill({ color: 0xe8b54a, alpha: 0.25 })
        lights.circle(lx, ly, 3.5).fill(0xf5d78e)
      }
      for (let i = 0; i < Math.floor(H); i++) {
        const [lx, ly] = pt(0, i + 0.5, -(wallH - 12 - (i % 2) * 6))
        lights.circle(lx, ly, 7).fill({ color: 0xe8b54a, alpha: 0.2 })
        lights.circle(lx, ly, 3.5).fill(0xecc978)
      }
    }
    root.addChild(lights)
  }

  // ── 책상 (deskCount만큼 — 에이전트보다 많으면 빈 책상, 'mix'면 종류 순환) ──
  slots.forEach(({ gx, gy }, k) => {
    const deskId = resolveDeskId(config.desk, k)
    if (placeKitSprite(root, `desk.${deskId}`, gx, gy, depth(gx + 1, gy + 0.5))) return
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
