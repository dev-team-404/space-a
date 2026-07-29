// 방 씬 렌더러 — 완성된 방 배경 프리셋(life.rN, 통짜 이미지) 위에 책상·로봇만 얹는다.
// 배경 이미지는 manifest cal(좌우 바닥 꼭짓점 + slope)로 2:1 격자 좌표계에 정규화된다.
// 책상은 킷 스프라이트(desk.dN)가 있으면 Sprite, 없으면 Graphics 아이소 박스로 폴백.

import { Assets, Container, Graphics, Sprite, Text, Texture, TextStyle } from 'pixi.js'
import type { SpaceAgent, SpaceIssue } from '../api'
import { DESKS, characterForSeed, resolveDeskId, resolveLifeId } from './catalog'
import { TILE_W, TILE_H, isoX, isoY, depth } from './iso'
import { kitPiece, kitPieceScale } from './kit'
import type { DeskId, LifeConfig } from './types'

// 0~2π 사이 결정적 위상 — 같은 agent는 항상 같은 위상으로 흔들려서, 여러 캐릭터가
// 전부 같은 박자로 움직이는 부자연스러움을 피한다 (characterForSeed와 동일한 해시 관례).
function seedPhase(seed: string): number {
  let h = 0
  for (let i = 0; i < seed.length; i++) h = (h * 31 + seed.charCodeAt(i)) | 0
  return (Math.abs(h) % 1000) / 1000 * Math.PI * 2
}

export type LifeSceneCallbacks = {
  onAgentTap?: (agent: SpaceAgent) => void
  /** 배경 칠판 영역 클릭 → 이슈 패널 */
  onBoardTap?: () => void
  /** 배경 책장 영역 클릭 → 지식 패널 */
  onShelfTap?: () => void
  /** 창문 클릭 → 창밖 잡담 말풍선. at = 창문 상단 중앙의 화면(canvas) 좌표 */
  onWindowTap?: (at: { x: number; y: number }) => void
  /** 책장 문서 건수 배지 클릭 → "이만큼 쌓였다" 말풍선. at = 배지 위쪽 화면 좌표 */
  onShelfBadgeTap?: (at: { x: number; y: number }) => void
  /** 정수기 클릭 → "물 한 잔 하고 하세요" 말풍선. at = 정수기 위쪽 화면 좌표 */
  onWaterTap?: (at: { x: number; y: number }) => void
}

export type LifeSceneData = {
  agents: SpaceAgent[]
  issues: SpaceIssue[]
  knowledgeCount: number
  /** 오늘의 하이라이트 한 줄 — 칠판에 분필 글씨로 표시 */
  highlight?: string | null
}

/** 빌더 프리뷰용 샘플 — 실데이터 fetch 없이 방 모양만 확인 */
export const PREVIEW_DATA: LifeSceneData = {
  agents: [
    { agent_id: 'p1', name: '김도현', role: '', owner: '', status: 'working', status_line: '', last_active_at: null },
    { agent_id: 'p2', name: '이하늘', role: '', owner: '', status: 'idle', status_line: '', last_active_at: null },
  ],
  issues: [
    { issue_id: 'i1', title: '배포 후 5xx 급증', status: 'open', opened_by: '', timeline: [] },
    { issue_id: 'i2', title: '인증서 문제', status: 'resolved', opened_by: '', timeline: [] },
  ],
  knowledgeCount: 12,
  highlight: '『배포 자동화 체크리스트』 지식이 플랫폼팀에서 재사용됐어요 (누적 4회)',
}

// Life 마스코트 이미지를 캐릭터로 쓸 때의 목표 높이(px) — 원본은 1000px 급이라 축소한다.
const MASCOT_H = 58

/** 씬별 캐릭터 표시 요소 — 경량 폴링(이름·아이콘만 갱신)이 씬을 다시 그리지 않도록 잡아둔다. */
type AgentView = { robot: Container; nameTag: Text; namePill: Graphics; mascotUrl: string }
const agentViews = new WeakMap<Container, Map<string, AgentView>>()

/** 이름표 텍스트·배경 폭을 다시 맞춘다(이름이 바뀌면 폭도 바뀐다). */
function paintNameTag(view: AgentView, name: string): void {
  view.nameTag.text = name
  const pillW = view.nameTag.width + 14
  const pillH = 20
  view.nameTag.position.set(-view.nameTag.width / 2, 4 + (pillH - view.nameTag.height) / 2)
  view.namePill
    .clear()
    .roundRect(-pillW / 2, 4, pillW, pillH, 9)
    .fill({ color: 0x0d1220, alpha: 0.92 })
    .stroke({ color: 0xffffff, alpha: 0.22, width: 1 })
}

/** 마스코트 이미지를 캐릭터 자리에 얹는다. 실패(404·네트워크)면 기존 캐릭터를 그대로 둔다. */
function applyMascot(view: AgentView, url: string): void {
  view.mascotUrl = url
  void Assets.load<Texture>({ src: url, loadParser: 'loadTextures' })
    .then((tex) => {
      if (view.robot.destroyed || !tex) return
      const sp = new Sprite(tex)
      sp.anchor.set(0.5, 1)
      sp.scale.set(MASCOT_H / tex.height)
      view.robot.addChildAt(sp, 0)
      // 기존 캐릭터/로봇 그림만 감춘다 — 이름표·말풍선은 살린다.
      view.robot.children.forEach((c) => {
        if (c !== sp && c !== view.namePill && (c instanceof Sprite || c instanceof Graphics)) {
          c.visible = false
        }
      })
    })
    .catch(() => {
      view.mascotUrl = '' // 다음 폴링에서 다시 시도할 수 있게
    })
}

/**
 * 경량 갱신 — 팀원이 이름·사진을 바꿨을 때 **씬을 다시 그리지 않고** 이름표·아이콘만 맞춘다.
 * 캐릭터 위치·말풍선·클릭 상태가 유지되는 것이 목적이다(공통 신원 스펙 §3.2).
 * 사람이 새로 들어오거나 나간 경우는 다루지 않는다 — 그건 방을 다시 열 때 반영된다.
 */
export function refreshAgentViews(scene: Container | null, agents: SpaceAgent[]): void {
  const views = scene ? agentViews.get(scene) : undefined
  if (!views) return
  for (const agent of agents) {
    const view = views.get(agent.agent_id)
    if (!view) continue
    if (agent.name && view.nameTag.text !== agent.name) paintNameTag(view, agent.name)
    if (agent.mascot_url && agent.mascot_url !== view.mascotUrl) applyMascot(view, agent.mascot_url)
  }
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
function placeKitSprite(root: Container, key: string, gx: number, gy: number, zIndex: number, mul = 1, yRatio = 1): Sprite | null {
  const piece = kitPiece(key)
  if (!piece || piece.mount !== 'floor') return null
  const [w, d] = piece.footprint
  const sp = new Sprite(piece.texture)
  sp.anchor.set(0.5, 1)
  const s = kitPieceScale() * mul
  sp.scale.set(s, s * yRatio)
  const [x, y] = pt(gx + w, gy + d)
  sp.position.set(x + piece.offset[0] * s, y + piece.offset[1] * s)
  sp.zIndex = zIndex
  root.addChild(sp)
  return sp
}

type Rect = [number, number, number, number]

/** a에서 b를 뺀 나머지 조각들(최대 4개). b가 없거나 안 겹치면 a 그대로.
 *  히트존 우선순위를 z순서가 아니라 **영역 분리**로 해결하는 데 쓴다. */
export function subtractRect(a: Rect, b?: Rect): Rect[] {
  if (!b) return [a]
  const [ax0, ay0, ax1, ay1] = a
  const ix0 = Math.max(ax0, b[0])
  const iy0 = Math.max(ay0, b[1])
  const ix1 = Math.min(ax1, b[2])
  const iy1 = Math.min(ay1, b[3])
  if (ix0 >= ix1 || iy0 >= iy1) return [a] // 교집합 없음
  const parts: Rect[] = []
  if (iy0 > ay0) parts.push([ax0, ay0, ax1, iy0]) // 위
  if (iy1 < ay1) parts.push([ax0, iy1, ax1, ay1]) // 아래
  if (ix0 > ax0) parts.push([ax0, iy0, ix0, iy1]) // 왼쪽
  if (ix1 < ax1) parts.push([ix1, iy0, ax1, iy1]) // 오른쪽
  return parts
}

function shade(color: number, f: number): number {
  const r = Math.min(255, Math.round(((color >> 16) & 0xff) * f))
  const g = Math.min(255, Math.round(((color >> 8) & 0xff) * f))
  const b = Math.min(255, Math.round((color & 0xff) * f))
  return (r << 16) | (g << 8) | b
}

// 책상 그리드: 가로 최대 4 × 세로 최대 3 = 12개. 1.7배로 커진 스프라이트가 겹치지
// 않도록 열·행 간격을 넉넉히 잡는다 (열 4.2칸, 행 3.8칸).
const DESK_COLS = 4
const DESK_COL_GAP = 4.2
const DESK_ROW_GAP = 3.8
const DESK_GX0 = 1.5
const DESK_GY0 = 2.5

/** 책상 배치 계산 — 줄을 균등 분배하고 각 줄을 중앙 정렬해, 추가될수록 가운데에서 퍼진다.
 *  예: 1개 = 중앙 하나, 2개 = 나란히, 5개 = 뒷줄 3 + 앞줄 2(가운데 정렬). */
function layoutDesks(count: number): { slots: { gx: number; gy: number }[]; W: number; H: number } {
  const rows = Math.max(1, Math.ceil(count / DESK_COLS))
  const base = Math.floor(count / rows)
  const extra = count % rows
  const rowSizes = Array.from({ length: rows }, (_, r) => base + (r < extra ? 1 : 0))
  const maxCols = Math.max(...rowSizes)
  const slots: { gx: number; gy: number }[] = []
  rowSizes.forEach((n, r) => {
    const off = (maxCols - n) / 2 // 짧은 줄은 반 칸씩 밀어 중앙 정렬
    for (let c = 0; c < n; c++) {
      slots.push({ gx: DESK_GX0 + (off + c) * DESK_COL_GAP, gy: DESK_GY0 + r * DESK_ROW_GAP })
    }
  })
  const W = Math.max(8, DESK_GX0 + maxCols * DESK_COL_GAP + 0.5)
  const H = Math.max(8, DESK_GY0 + rows * DESK_ROW_GAP + 1.5)
  return { slots, W, H }
}

export function buildLifeScene(
  config: LifeConfig,
  data: LifeSceneData,
  cb: LifeSceneCallbacks = {},
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

  // ── 방 배경 프리셋 (life.rN — 벽·바닥·책장·칠판이 다 그려진 통짜 이미지 한 장) ──
  // 캘리브레이션(manifest cal)으로 이미지를 2:1 격자 좌표계에 정규화한다:
  // 좌우 바닥 꼭짓점 ↔ 격자 (0,W)/(W,0), 이미지 기울기(slope)는 비등방 스케일로 보정.
  const lifeId = resolveLifeId(config.life)
  const lifeKit = kitPiece(`life.${lifeId}`) ?? kitPiece('shell.life')
  const lifeCal = lifeKit?.cal
  let cut = 0 // 뒷벽이 잘라먹는 격자 깊이: gx+gy < cut 영역은 벽 뒤
  if (lifeKit && lifeCal) {
    const [lxI, lyI] = lifeCal.left
    const [rxI] = lifeCal.right
    const cxI = (lxI + rxI) / 2
    const ax = (rxI - lxI) / (2 * W) // 이미지 px / 격자 x단위
    const ay = ax * (lifeCal.slope ?? 0.5)
    const cyI = lyI - W * ay // 가상 다이아 원점의 이미지 y
    const sx = TILE_W / 2 / ax
    const sy = TILE_H / 2 / ay
    const sp = new Sprite(lifeKit.texture)
    sp.scale.set(sx, sy)
    sp.position.set(-cxI * sx, -cyI * sy)
    sp.zIndex = -1000
    root.addChild(sp)
    cut = (lifeCal.backEdgeY - cyI) / ay

    // 배경에 이미 그려진 칠판·책장 영역에 투명 히트존 → 이슈/지식 패널 진입점 유지.
    // cal 좌표(이미지 px)를 배경 스프라이트와 같은 변환(sx/sy, -cxI/-cyI)으로 화면에 맞춘다.
    // URL에 hitzones가 있으면 히트존을 색으로 칠하고 탭을 콘솔에 찍는다 — 클릭이 어느 영역에
     // 먹는지 화면에서 바로 보기 위한 디버그 스위치 (예: /?hitzones=1#life/sw-innov).
    const debugZones = typeof location !== 'undefined' && location.href.includes('hitzones')
    const hitZone = (
      area: [number, number, number, number],
      z: number,
      onTap?: () => void,
      label = '',
    ) => {
      if (!onTap) return
      const [x0, y0, x1, y1] = area
      const tint = label === 'window' ? 0xff3b30 : label === 'shelf' ? 0x00e5ff : 0xffd60a
      const hz = new Graphics()
        .rect((x0 - cxI) * sx, (y0 - cyI) * sy, (x1 - x0) * sx, (y1 - y0) * sy)
        // 투명하지만 히트 판정은 살아 있게. 디버그일 때만 눈에 보이게 칠한다.
        .fill(debugZones ? { color: tint, alpha: 0.3 } : { color: 0xffffff, alpha: 0.001 })
      hz.zIndex = z
      hz.eventMode = 'static'
      hz.cursor = 'pointer'
      hz.on('pointertap', () => {
        if (debugZones) console.log(`[hitzone] ${label || '?'} tapped`, area)
        onTap()
      })
      root.addChild(hz)
    }
    hitZone(lifeCal.backWall, -800, cb.onBoardTap, 'board')

    // ── 책장 위 문서 건수 배지 — 클릭하면 "이만큼 쌓였다" 말풍선 ──
    // 배지를 먼저 만든다: 아래에서 책장 히트존에서 배지 영역을 빼야 하기 때문이다.
    // (배지도 책장 영역 안에 있어, 겹치면 배지 클릭을 책장이 먹는다 — 창문과 같은 문제)
    let badgeRect: Rect | undefined
    let badgeAnchor: { x: number; y: number } | undefined
    if (lifeCal.shelfArea && data.knowledgeCount > 0) {
      const [bx0, by0, bx1] = lifeCal.shelfArea
      const badge = new Text({
        text: `📑 문서 ${data.knowledgeCount}건`,
        style: new TextStyle({
          fill: 0xf3e6c8,
          fontSize: 13,
          fontWeight: '700',
          fontFamily: '"Galmuri11", "Apple SD Gothic Neo", monospace',
          dropShadow: { color: 0x1a1005, alpha: 0.9, blur: 4, distance: 0, angle: 0 },
        }),
      })
      badge.anchor.set(0.5, 1)
      // 책장 영역 안쪽으로 내려 붙인다 — 벽에 떠 있으면 책장 것인지 애매하다.
      badge.position.set(((bx0 + bx1) / 2 - cxI) * sx, (by0 - cyI) * sy + 250 * sy)
      badge.scale.set(Math.min(1, ((bx1 - bx0) * sx * 0.9) / badge.width))
      badge.zIndex = -780
      root.addChild(badge)
      badgeAnchor = { x: badge.x, y: badge.y - badge.height }
      // 배지 화면 사각형을 이미지 좌표로 되돌린다(px 여유 8) — 히트존·영역 차집합의 단위가 이미지 px.
      const toImg = (lx: number, ly: number): [number, number] => [lx / sx + cxI, ly / sy + cyI]
      const [ix0, iy0] = toImg(badge.x - badge.width / 2, badge.y - badge.height)
      const [ix1, iy1] = toImg(badge.x + badge.width / 2, badge.y)
      badgeRect = [ix0 - 8, iy0 - 6, ix1 + 8, iy1 + 6]
    }

    // 책장 영역에서 창문·배지 사각형을 빼고 남은 조각만 히트존으로 쓴다. 배경상 창문 아래에
    // 사이드보드가 있어 두 영역이 겹치는데, 겹치면 사람이 창문 유리를 눌러도 책장이 먹었다
    // (2026-07-29). z순서에 기대지 않고 영역 자체를 분리해 순서 의존을 없앤다.
    if (lifeCal.shelfArea) {
      // 창문·배지·정수기 순으로 구멍을 뚫는다 — 겹친 채로 두면 그 오브젝트 클릭을 책장이 먹는다.
      const holes = [lifeCal.windowArea, badgeRect, lifeCal.waterArea]
      const parts = holes.reduce<Rect[]>(
        (acc, hole) => acc.flatMap((p) => subtractRect(p, hole)),
        [lifeCal.shelfArea],
      )
      for (const part of parts) hitZone(part, -800, cb.onShelfTap, 'shelf')
    }
    if (badgeRect && badgeAnchor && cb.onShelfBadgeTap) {
      const anchor = badgeAnchor
      hitZone(badgeRect, -790, () => cb.onShelfBadgeTap?.(root.toGlobal(anchor)), 'badge')
    }

    // ── 정수기 — 클릭하면 "물 한 잔 하고 하세요" 말풍선 ──
    if (lifeCal.waterArea && cb.onWaterTap) {
      const [ax0, ay0, ax1] = lifeCal.waterArea
      const waterAnchor = { x: ((ax0 + ax1) / 2 - cxI) * sx, y: (ay0 - cyI) * sy }
      hitZone(lifeCal.waterArea, -795, () => cb.onWaterTap?.(root.toGlobal(waterAnchor)), 'water')
    }

    // ── 창문 — 클릭하면 창밖을 두고 주고받는 잡담 말풍선 ──
    // 창문 아래쪽은 shelfArea와 겹친다(창 밑에 사이드보드가 있는 배경). 사람이 실제로 누르는
    // 곳은 밝은 유리 한가운데라 **창문이 이겨야** 한다 — zIndex를 책장(-800)보다 위로 둔다.
    // 말풍선은 DOM 오버레이라 화면 좌표가 필요하다. 씬 배율·위치는 fitScene이 매번 바꾸므로
    // 미리 계산해 두지 않고 **탭 시점에** toGlobal로 구한다.
    if (lifeCal.windowArea && cb.onWindowTap) {
      const [wx0, wy0, wx1] = lifeCal.windowArea
      const anchorLocal = { x: ((wx0 + wx1) / 2 - cxI) * sx, y: (wy0 - cyI) * sy }
      hitZone(
        lifeCal.windowArea,
        -795,
        () => cb.onWindowTap?.(root.toGlobal(anchorLocal)),
        'window',
      )
    }

    // ── 칠판 위 나무 간판에 스페이스 이름 ──
    // 간판 안쪽 영역(signArea)은 프리셋마다 다르다(manifest cal). 배경과 같은 변환으로 얹는다.
    const name = config.space_name?.trim()
    if (name && lifeCal.signArea) {
      const [sax0, say0, sax1, say1] = lifeCal.signArea
      const SIGN_CX = (sax0 + sax1) / 2 // 간판 중앙 x(이미지 px)
      const SIGN_CY = (say0 + say1) / 2 // 간판 중앙 y(이미지 px)
      const PAD = 20 // 좌우 안쪽 패딩(이미지 px)
      const SIGN_W = Math.max(40, sax1 - sax0 - PAD * 2) // 텍스트 안전 폭
      const SIGN_H = Math.max(20, say1 - say0 - 8) // 텍스트 안전 높이
      const label = new Text({
        text: name,
        style: new TextStyle({
          fill: 0x241505, // 검정에 가까운 글씨
          fontSize: 32, // 기준 크기 — 아래에서 간판에 맞게 축소만 한다
          fontWeight: '700',
          fontFamily: '"Apple SD Gothic Neo", "Noto Sans KR", system-ui, sans-serif',
          // 흰 그림자(사방 균일 — distance 0)로 어두운 나무판 위에서도 또렷하게. 아래 치우침 없음.
          dropShadow: { color: 0xfff6e6, alpha: 0.95, blur: 3, distance: 0, angle: 0 },
        }),
      })
      label.anchor.set(0.5)
      // 간판 안쪽 영역(폭·높이)을 넘지 않게 등비 축소. 짧으면 기준 크기 유지.
      const maxW = SIGN_W * sx
      const maxH = SIGN_H * sy
      const fit = Math.min(1, maxW / label.width, maxH / label.height)
      label.scale.set(fit)
      label.position.set((SIGN_CX - cxI) * sx, (SIGN_CY - cyI) * sy)
      label.zIndex = -900 // 배경(-1000) 위, 히트존/책상 아래
      root.addChild(label)
    }

    // ── 칠판에 오늘의 하이라이트 — 하루치 기록 중 가장 핵심인 사건 한 줄 (분필 느낌) ──
    const hl = data.highlight?.trim()
    if (hl && lifeCal.backWall) {
      const [bx0, by0, bx1, by1] = lifeCal.backWall
      // 칠판 나무 프레임 안쪽 여백 (이미지 px)
      const PADX = 36
      const PADY = 26
      const boardX = (bx0 + PADX - cxI) * sx
      const boardY = (by0 + PADY - cyI) * sy
      const boardW = (bx1 - bx0 - PADX * 2) * sx
      const boardH = (by1 - by0 - PADY * 2) * sy

      const font = '"Apple SD Gothic Neo", "Noto Sans KR", system-ui, sans-serif'
      const headerStyle = (size: number) =>
        new TextStyle({
          fill: 0xffd97a, // 노란 분필
          fontSize: size,
          fontWeight: '700',
          fontFamily: font,
          dropShadow: { color: 0xffd97a, alpha: 0.35, blur: 4, distance: 0, angle: 0 }, // 분필 번짐
        })
      const bodyStyle = (size: number) =>
        new TextStyle({
          fill: 0xf3f0e2, // 흰 분필
          fontSize: size,
          fontWeight: '600',
          lineHeight: Math.round(size * 1.45),
          fontFamily: font,
          wordWrap: true,
          wordWrapWidth: boardW,
          breakWords: true, // 한국어 긴 제목도 칠판 폭에서 강제 줄바꿈
          dropShadow: { color: 0xf3f0e2, alpha: 0.3, blur: 3, distance: 0, angle: 0 },
        })
      const header = new Text({ text: '★ 오늘의 하이라이트', style: headerStyle(21) })
      const body = new Text({ text: hl, style: bodyStyle(20) })
      // 칠판 폭 전체로 줄바꿈하며 들어가는 최대 본문 폰트 크기를 이진 탐색 —
      // 등비 축소 방식은 세로 초과 시 폭까지 좁아져 칠판 왼쪽만 쓰게 된다.
      const gapY = 10
      const totalH = () => header.height + gapY + body.height
      let lo = 12
      let hi = 44
      let best = lo
      while (lo <= hi) {
        const mid = Math.floor((lo + hi) / 2)
        header.style = headerStyle(mid + 2)
        body.style = bodyStyle(mid)
        if (totalH() <= boardH && header.width <= boardW) {
          best = mid
          lo = mid + 1
        } else {
          hi = mid - 1
        }
      }
      header.style = headerStyle(best + 2)
      body.style = bodyStyle(best)

      const chalk = new Container()
      body.position.set(0, header.height + gapY)
      // 밑줄 — 분필로 그은 구분선
      const rule = new Graphics()
        .moveTo(0, header.height + 3)
        .lineTo(Math.min(boardW, header.width + 24), header.height + 3)
        .stroke({ color: 0xf3f0e2, alpha: 0.5, width: 2 })
      chalk.addChild(header, rule, body)
      chalk.position.set(boardX, boardY + Math.max(0, (boardH - totalH()) / 2))
      chalk.zIndex = -890 // 간판(-900) 위, 히트존(-800) 아래
      root.addChild(chalk)
    }
  }

  // 책상 클러스터는 방 중앙에 — 단, 뒷벽 컷 안쪽으로 (gx+gy ≥ cut)
  const dx = (W - LW) / 2
  const dy = Math.max(1.2, (H - LH) / 2, cut + 2 - 4 - (W - LW) / 2)
  const slots = baseSlots.map((s) => ({ gx: s.gx + dx, gy: s.gy + dy }))

  // ── 책상 (deskCount만큼 — 에이전트보다 많으면 빈 책상, 'mix'면 종류 순환) ──
  // 새 desk 스프라이트가 시트보다 작게 잘려 나와, 배경 대비 살짝 키워 얹는다.
  const DESK_SCALE = 1.7
  // desks.png 원본 비율 그대로 렌더링한다(세로 눌림 없음).
  const DESK_Y_RATIO = 1
  slots.forEach(({ gx, gy }, k) => {
    const deskId = resolveDeskId(config.desk, k)
    if (placeKitSprite(root, `desk.${deskId}`, gx, gy, depth(gx + 1, gy + 0.5), DESK_SCALE, DESK_Y_RATIO)) return
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

  // ── 에이전트 — 책상에 배치하고, 자리가 모자라면 방 앞쪽에 서 있음 ──
  // 에이전트마다 15종 캐릭터를 결정적으로 랜덤 배정(agent_id 기반)해 다양하게 보인다.
  // 킷이 못 뜨면 Graphics 로봇으로 폴백한다.
  const nameStyle = new TextStyle({
    fill: 0xffffff,
    fontSize: 13,
    fontWeight: '600',
    fontFamily: '"Apple SD Gothic Neo", "Noto Sans KR", system-ui, sans-serif',
    dropShadow: { color: 0x000000, alpha: 0.8, blur: 2, distance: 0, angle: 0 },
  })
  const CHAR_SCALE = 1.0 // kitPieceScale 위에 곱하는 배수 — 책상(1.7배)과 어울리게, 의자에 앉은 크기
  const makeRobot = (agent: SpaceAgent, k: number, rgx: number, rgy: number) => {
    const robot = new Container()
    const [rx, ry] = pt(rgx, rgy)
    robot.position.set(rx, ry)
    robot.zIndex = depth(rgx, rgy)
    // 캐릭터 발끝이 격자 지점(원점)에 닿도록 앵커 하단 중앙. topY = 캐릭터 머리 위 y(음수).
    let topY = -34
    // 공통 신원 — Life에 마스코트 이미지가 있으면 그 사람의 아이콘으로 세운다(절차 생성보다 우선).
    // 스펙: docs/design/common/specs/2026-07-29-shared-identity-life-hub-lens.md
    // 로드는 비동기라 먼저 기본 캐릭터를 세우고, 도착하면 갈아끼운다(실패하면 기본 그대로).
    const charKit = kitPiece(`char.${characterForSeed(agent.agent_id || agent.name || String(k))}`)
    if (charKit && charKit.mount === 'character') {
      const s = kitPieceScale() * CHAR_SCALE
      const sp = new Sprite(charKit.texture)
      sp.anchor.set(0.5, 1)
      sp.scale.set(s)
      robot.addChild(sp)
      topY = -charKit.texture.height * s
    } else {
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
      topY = -43
    }
    const nameTag = new Text({ text: agent.name, style: nameStyle })
    // 앞줄 책상 스프라이트·캐릭터 위에서도 또렷하게 읽히도록 짙은 필 + 테두리
    const pillW = nameTag.width + 14
    const pillH = 20
    nameTag.position.set(-nameTag.width / 2, 4 + (pillH - nameTag.height) / 2)
    const namePill = new Graphics()
      .roundRect(-pillW / 2, 4, pillW, pillH, 9)
      .fill({ color: 0x0d1220, alpha: 0.92 })
      .stroke({ color: 0xffffff, alpha: 0.22, width: 1 })
    robot.addChild(namePill, nameTag)
    // 폴링 갱신용 등록 + Life 마스코트가 있으면 지금 얹는다(비동기, 실패 시 기본 캐릭터 유지).
    const view: AgentView = { robot, nameTag, namePill, mascotUrl: '' }
    let views = agentViews.get(root)
    if (!views) {
      views = new Map()
      agentViews.set(root, views)
    }
    views.set(agent.agent_id, view)
    if (agent.mascot_url) applyMascot(view, agent.mascot_url)
    // 작업 중이면 머리 위에 말풍선 — 최근 활동 요약(brief)이 있으면 그 문구를, 없으면 '...'
    if (agent.status === 'working') {
      const brief = agent.recent_activity?.brief
      const by = topY - 8
      if (brief) {
        // 말풍선은 한 줄만 — 넘치면 '…'로 자른다 (Pixi Text엔 CSS ellipsis가 없어 직접 계산).
        const bubbleStyle = new TextStyle({
          fill: 0x3a2f1a,
          fontSize: 11,
          fontWeight: '600',
          fontFamily: '"Apple SD Gothic Neo", "Noto Sans KR", system-ui, sans-serif',
        })
        const MAX_W = 150
        const txt = new Text({ text: brief, style: bubbleStyle })
        if (txt.width > MAX_W) {
          // 들어갈 최대 길이를 이진 탐색으로 — 한 글자씩 지우며 매번 측정하는 O(N)을 O(log N)으로.
          let low = 0
          let high = brief.length
          let best = '…'
          while (low <= high) {
            const mid = Math.floor((low + high) / 2)
            txt.text = brief.slice(0, mid) + '…'
            if (txt.width <= MAX_W) {
              best = txt.text
              low = mid + 1
            } else {
              high = mid - 1
            }
          }
          txt.text = best
        }
        txt.anchor.set(0.5, 1)
        const padX = 8
        const padY = 5
        const bw = txt.width + padX * 2
        const bh = txt.height + padY * 2
        const cx = 0 // 캐릭터 머리 중앙 위
        const bub = new Graphics()
        bub.roundRect(cx - bw / 2, by - bh, bw, bh, 8).fill(0xf0e6d2)
        bub.poly([cx - 5, by - 1, cx + 5, by - 1, cx, by + 6]).fill(0xf0e6d2) // 꼬리
        txt.position.set(cx, by - padY)
        robot.addChild(bub, txt)
      } else {
        const bub = new Graphics()
        bub.roundRect(12, by - 14, 30, 16, 8).fill(0xf0e6d2)
        bub.poly([16, by + 1, 24, by + 1, 15, by + 8]).fill(0xf0e6d2)
        bub.circle(20, by - 6, 1.8).fill(0x555)
        bub.circle(27, by - 6, 1.8).fill(0x555)
        bub.circle(34, by - 6, 1.8).fill(0x555)
        robot.addChild(bub)
      }
    }
    // 오프라인(idle) 에이전트는 유령처럼 반투명하게 — 방에 있지만 지금은 활동 중이 아님을 표시.
    if (agent.status !== 'working') {
      robot.alpha = 0.4
    }
    // idle 흔들림 — 정지 화면이 죽어 보이지 않게 위아래로 살짝 떠 있는 느낌만 준다.
    // 작업 중이면 조금 더 활기차게, 오프라인(유령)은 느리고 옅게 — 상태 구분을 움직임으로도 보강.
    const phase = seedPhase(agent.agent_id || agent.name || String(k))
    const amp = agent.status === 'working' ? 3 : 1.2
    const speed = agent.status === 'working' ? 1.8 : 0.9
    robot.onRender = () => {
      robot.y = ry + Math.sin(performance.now() / 1000 * speed + phase) * amp
    }
    robot.eventMode = 'static'
    robot.cursor = 'pointer'
    robot.on('pointertap', () => cb.onAgentTap?.(agent))
    root.addChild(robot)
  }
  // 캐릭터는 의자에 앉은 위치(책상 쪽으로 더 붙이고 화면 오른쪽=의자 쪽으로).
  const seatDx = 2.3
  const seatDy = 0.35
  // flip된 책상(d3·d6·d7·d9)은 의자가 반대편이라 시트를 보정: 왼쪽 2보·위 0.5보.
  // 격자에서 왼쪽 이동 = gx-·gy+, 위 이동 = gx-·gy-. 조합해 dgx=-1.25, dgy=+1.75.
  const SEAT_FIX: Partial<Record<DeskId, [number, number]>> = {
    d3: [-1.1, 0.5], d6: [-1.1, 0.5], d7: [-1.1, 0.5], d9: [-1.1, 0.5],
  }
  data.agents.forEach((agent, k) => {
    const slot = slots[k]
    if (slot) {
      const [fx, fy] = SEAT_FIX[resolveDeskId(config.desk, k)] ?? [0, 0]
      makeRobot(agent, k, slot.gx + seatDx + fx, slot.gy + seatDy + fy) // 책상 의자 자리
    } else makeRobot(agent, k, dx + 1.5 + ((k - slots.length) % 5) * 1.7, dy + LH - 1.2) // 서 있음
  })

  return root
}
