// 방 씬 렌더러 — RoomConfig(격자 + variant 데이터)만으로 방을 그린다.
// 이미지 캘리브레이션 없음: 가구는 전부 Graphics 프리미티브 아이소 박스.
// 스프라이트 에셋이 생기면 drawIsoBox 자리가 Sprite로 바뀌고 좌표계는 그대로 유지된다.

import { Container, Graphics, Text, TextStyle } from 'pixi.js'
import type { SpaceAgent, SpaceIssue } from '../api'
import { BOARDS, DESKS, FLOORS, WALLPAPERS } from './catalog'
import { TILE_W, TILE_H, WALL_H, isoX, isoY, depth } from './iso'
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
  const W = Math.max(9, 1.5 + cols * 3 + 1)
  const H = Math.max(9, 2.5 + rows * 3 + 3)
  return { slots, W, H }
}

export function buildRoomScene(
  config: RoomConfig,
  data: RoomSceneData,
  cb: RoomSceneCallbacks = {},
): Container {
  const root = new Container()
  root.sortableChildren = true

  const wall = WALLPAPERS[config.wallpaper]
  const floor = FLOORS[config.floor]
  const desk = DESKS[config.desk]
  const board = BOARDS[config.board]

  const deskCount = Math.max(1, data.agents.length)
  const { slots, W, H } = layoutDesks(deskCount)

  // ── 바닥 ──
  const floorG = new Graphics()
  floorG.zIndex = -1000
  for (let gx = 0; gx < W; gx++) {
    for (let gy = 0; gy < H; gy++) {
      const alt =
        config.floor === 'checker' ? (gx + gy) % 2 === 0 : config.floor === 'plank' ? gy % 2 === 0 : (gx * 7 + gy * 3) % 5 < 3
      const [tx, ty] = pt(gx, gy)
      floorG
        .poly([tx, ty, tx + TILE_W / 2, ty + TILE_H / 2, tx, ty + TILE_H, tx - TILE_W / 2, ty + TILE_H / 2])
        .fill(alt ? floor.a : floor.b)
        .stroke({ color: floor.edge, width: 1, alpha: 0.35 })
    }
  }
  // 바닥 테두리(마감)
  floorG
    .poly([...pt(0, 0), ...pt(W, 0), ...pt(W, H), ...pt(0, H)])
    .stroke({ color: floor.edge, width: 3, alpha: 0.9 })
  root.addChild(floorG)

  // ── 뒷벽 두 면 ──
  const wallsG = new Graphics()
  wallsG.zIndex = -900
  // 오른쪽 뒷벽 (gy=0 면)
  wallsG.poly([...pt(0, 0, -WALL_H), ...pt(W, 0, -WALL_H), ...pt(W, 0), ...pt(0, 0)]).fill(wall.wall)
  // 왼쪽 뒷벽 (gx=0 면)
  wallsG.poly([...pt(0, 0, -WALL_H), ...pt(0, H, -WALL_H), ...pt(0, H), ...pt(0, 0)]).fill(wall.shade)
  // 걸레받이 트림
  wallsG.poly([...pt(0, 0, -10), ...pt(W, 0, -10), ...pt(W, 0), ...pt(0, 0)]).fill(wall.trim)
  wallsG.poly([...pt(0, 0, -10), ...pt(0, H, -10), ...pt(0, H), ...pt(0, 0)]).fill(shade(wall.trim, 0.85))
  root.addChild(wallsG)

  // ── 칠판 (오른쪽 뒷벽, gy=0 면) — 클릭 → 이슈 패널 ──
  const boardG = new Graphics()
  boardG.zIndex = -890
  const bA = 1.2 // 벽을 따라 시작 cell
  const bB = Math.min(W - 1, bA + 4.6)
  const bTop = WALL_H - 18
  const bBot = 38
  boardG
    .poly([...pt(bA, 0, -bTop), ...pt(bB, 0, -bTop), ...pt(bB, 0, -bBot), ...pt(bA, 0, -bBot)])
    .fill(board.face)
    .stroke({ color: board.frame, width: 5 })
  boardG.eventMode = 'static'
  boardG.cursor = 'pointer'
  boardG.on('pointertap', () => cb.onBoardTap?.())
  root.addChild(boardG)

  // 칠판 내용 — 이슈 상위 3건 (벽면 기울기에 맞춰 skew)
  const wallSkew = Math.atan2(TILE_H / 2, TILE_W / 2)
  const chalkStyle = new TextStyle({ fill: board.chalk, fontSize: 13 })
  data.issues.slice(0, 3).forEach((issue, i) => {
    const line = new Container()
    const [lx, ly] = pt(bA + 0.35, 0, -(bTop - 22 - i * 24))
    line.position.set(lx, ly)
    line.skew.y = wallSkew
    line.zIndex = -889
    const dot = new Graphics().circle(6, 7, 5).fill(ISSUE_DOT[issue.status] ?? 0xd9a441)
    const label = new Text({
      text: issue.title.length > 16 ? issue.title.slice(0, 16) + '…' : issue.title,
      style: chalkStyle,
    })
    label.x = 16
    line.addChild(dot, label)
    root.addChild(line)
  })
  if (data.issues.length === 0) {
    const empty = new Text({ text: '이슈 없음', style: chalkStyle })
    const [ex, ey] = pt(bA + 0.35, 0, -(bTop - 26))
    empty.position.set(ex, ey)
    empty.skew.y = wallSkew
    empty.zIndex = -889
    root.addChild(empty)
  }

  // ── 책장 (왼쪽 뒷벽 앞) — 클릭 → 지식 패널 ──
  const shelfG = new Graphics()
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
  shelfG.eventMode = 'static'
  shelfG.cursor = 'pointer'
  shelfG.on('pointertap', () => cb.onShelfTap?.())
  root.addChild(shelfG)

  const countStyle = new TextStyle({ fill: 0xf0e6d2, fontSize: 12, fontWeight: 'bold' })
  const shelfBadge = new Text({ text: `지식 ${data.knowledgeCount}`, style: countStyle })
  const [sbx, sby] = pt(0.5, 2.2, -118)
  shelfBadge.position.set(sbx - shelfBadge.width / 2, sby)
  shelfBadge.zIndex = depth(1, 2) + 0.1
  root.addChild(shelfBadge)

  // ── 장식 ──
  if (config.deco.includes('rug')) {
    const rug = new Graphics()
    const rc = { gx: W * 0.62, gy: H * 0.62 }
    rug
      .poly([...pt(rc.gx - 1.4, rc.gy - 1.4), ...pt(rc.gx + 1.4, rc.gy - 1.4), ...pt(rc.gx + 1.4, rc.gy + 1.4), ...pt(rc.gx - 1.4, rc.gy + 1.4)])
      .fill({ color: 0xb0524a, alpha: 0.9 })
      .stroke({ color: 0x8a3e38, width: 3 })
    rug.zIndex = -800
    root.addChild(rug)
  }
  if (config.deco.includes('plant')) {
    const plantAt = (gx: number, gy: number) => {
      const p = new Graphics()
      drawIsoBox(p, gx, gy, 0.55, 0.55, 16, { top: 0x8a5b2e, left: 0x6b4423, right: 0x7a4e28 })
      const [px, py] = pt(gx + 0.28, gy + 0.28, -16)
      p.circle(px, py - 16, 13).fill(0x4e7a3a)
      p.circle(px - 10, py - 8, 9).fill(0x5d8c46)
      p.circle(px + 10, py - 9, 10).fill(0x446b33)
      p.zIndex = depth(gx, gy)
      root.addChild(p)
    }
    plantAt(W - 1.4, 0.6)
    plantAt(0.6, H - 1.6)
  }
  if (config.deco.includes('water-cooler')) {
    const wc = new Graphics()
    const wgx = W - 1.3
    const wgy = 1.0
    drawIsoBox(wc, wgx, wgy, 0.6, 0.6, 46, { top: 0xc9d2d8, left: 0xa9b4bc, right: 0xb8c4cc })
    const [wx, wy] = pt(wgx + 0.3, wgy + 0.3, -46)
    wc.ellipse(wx, wy - 10, 10, 12).fill({ color: 0x5b9bd5, alpha: 0.9 })
    wc.zIndex = depth(wgx, wgy)
    root.addChild(wc)
  }
  if (config.deco.includes('string-lights')) {
    const lights = new Graphics()
    lights.zIndex = -880
    const n = Math.floor(W)
    for (let i = 0; i < n; i++) {
      const [lx, ly] = pt(i + 0.5, 0, -(WALL_H - 12 - (i % 2) * 6))
      lights.circle(lx, ly, 7).fill({ color: 0xe8b54a, alpha: 0.25 })
      lights.circle(lx, ly, 3.5).fill(0xf5d78e)
    }
    for (let i = 0; i < Math.floor(H); i++) {
      const [lx, ly] = pt(0, i + 0.5, -(WALL_H - 12 - (i % 2) * 6))
      lights.circle(lx, ly, 7).fill({ color: 0xe8b54a, alpha: 0.2 })
      lights.circle(lx, ly, 3.5).fill(0xecc978)
    }
    root.addChild(lights)
  }

  // ── 책상 + 로봇 (에이전트 수만큼) ──
  const nameStyle = new TextStyle({ fill: 0xe8eaed, fontSize: 11 })
  data.agents.forEach((agent, k) => {
    const slot = slots[k]
    if (!slot) return
    const { gx, gy } = slot

    const dg = new Graphics()
    drawIsoBox(dg, gx, gy, 2, 1, 34, { top: desk.top, left: shade(desk.side, 0.9), right: desk.side })
    // 모니터
    const [mx, my] = pt(gx + 1.0, gy + 0.35, -34)
    dg.poly([mx - 14, my - 30, mx + 14, my - 16, mx + 14, my + 4, mx - 14, my - 10]).fill(0x23262b)
    dg.poly([mx - 11, my - 26, mx + 11, my - 15, mx + 11, my - 1, mx - 11, my - 12]).fill(0x3a4550)
    dg.zIndex = depth(gx + 1, gy + 0.5)
    root.addChild(dg)

    // 로봇 — 책상 앞(gy+1.6)에 앉음. 클릭 → 에이전트 패널
    const rgx = gx + 1
    const rgy = gy + 1.6
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
    robot.addChild(nameTag)
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
  })

  return root
}
