// 스프라이트 킷 — /assets/kit/manifest.json에 등록된 부품 PNG를 로드한다.
// 킷에 있는 부품은 renderer가 Sprite로 그리고, 없으면 Graphics 프리미티브로 폴백한다.
// → PNG를 assets/kit/에 넣고 manifest에 한 줄 추가하면 화면 퀄리티가 그 자리에서 올라간다.
// 제작 스펙: a-lens/assets/kit/README.md

import { Assets, Texture } from 'pixi.js'

export type KitMount = 'floor' | 'wall-right'

export type KitPiece = {
  texture: Texture
  /** 격자 footprint [w(gx방향), d(gy방향)] — floor 부품용 */
  footprint: [number, number]
  /** 앵커 미세 보정 px (에셋 원본 픽셀 기준) */
  offset: [number, number]
  mount: KitMount
}

type ManifestPiece = {
  file: string
  footprint?: [number, number]
  offset?: [number, number]
  mount?: KitMount
}

type Manifest = {
  /** 에셋 원본 px → 씬 px 배율 (1 tile = TILE_W px 기준). 예: 4x 에셋이면 0.25 */
  scale?: number
  pieces?: Record<string, ManifestPiece>
}

const BASE = '/assets/kit/'

let pieces = new Map<string, KitPiece>()
let kitScale = 0.25
let loaded = false

/** 앱 시작 시 1회. manifest가 없으면(404) 빈 킷 — 전부 Graphics 폴백. */
export async function loadKit(): Promise<void> {
  if (loaded) return
  loaded = true
  let manifest: Manifest
  try {
    const res = await fetch(BASE + 'manifest.json')
    if (!res.ok) return
    manifest = (await res.json()) as Manifest
  } catch {
    return
  }
  kitScale = manifest.scale ?? 0.25
  const entries = Object.entries(manifest.pieces ?? {})
  await Promise.all(
    entries.map(async ([key, p]) => {
      try {
        const texture = await Assets.load<Texture>(BASE + p.file)
        texture.source.scaleMode = 'nearest' // 픽셀아트 — 보간 없이
        pieces.set(key, {
          texture,
          footprint: p.footprint ?? [1, 1],
          offset: p.offset ?? [0, 0],
          mount: p.mount ?? 'floor',
        })
      } catch (e) {
        console.warn(`킷 부품 로드 실패: ${key} (${p.file})`, e)
      }
    }),
  )
}

export function kitPiece(key: string): KitPiece | undefined {
  return pieces.get(key)
}

export function kitPieceScale(): number {
  return kitScale
}
