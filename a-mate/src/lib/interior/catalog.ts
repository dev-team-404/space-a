import assetMetricsJson from './asset-metrics.json';

export type FurnitureCategory = 'sofa' | 'lighting' | 'desk' | 'table' | 'chair' | 'appliance' | 'window';
export type Rotation = 0 | 90 | 180 | 270;
export type Direction = 'ne' | 'se' | 'sw' | 'nw';
export type SpriteSource = 'ne' | 'sw';
export type WallSide = 'north' | 'west';
export type FootprintCell = [number, number];

export type OrientationMode = 'directed' | 'axial' | 'invariant';
export interface TurnaroundSpec { orientation: OrientationMode; sources: Partial<Record<SpriteSource, Direction>>; }
export interface ResolvedSpriteView { source: SpriteSource; mirrorX: boolean; facing: Direction; }
export interface SpriteProjection { scaleY: number; shearY: number; }
export interface SpriteRenderSpec { widthTiles: Record<Rotation, number>; anchors: Record<Rotation, [number, number]>; footprintAnchor: [number, number]; mirrorX: Record<Rotation, boolean>; projections: Record<Rotation, SpriteProjection>; sources: Record<Rotation, SpriteSource>; }
export interface FurnitureItem { id: string; category: FurnitureCategory; name: string; sprites: Record<Rotation, string>; size: [number, number]; footprint: FootprintCell[]; render: SpriteRenderSpec; turnaround?: TurnaroundSpec; }
export interface PlacedFurniture { asset_id: string; category: FurnitureCategory | string; cell: [number, number]; size: [number, number]; footprint?: FootprintCell[]; rotation: Rotation; wall?: WallSide | null; }
export interface InteriorTheme { id: string; name: string; wallpaper: string; floor: string; wall: string; wallSide: string; floorBase: string; floorAlt: string; grout: string; }

export const ROTATIONS: Rotation[] = [0, 90, 180, 270];
export const DIRECTIONS: Direction[] = ['ne', 'se', 'sw', 'nw'];
export const CATEGORY_LABELS: Record<FurnitureCategory, string> = { sofa: '소파', lighting: '조명', desk: '책상', table: '테이블', chair: '의자', appliance: '가전', window: '창문' };
export const SPRITE_DIRECTION_BY_ROTATION: Record<Rotation, Direction> = { 0: 'ne', 90: 'se', 180: 'sw', 270: 'nw' };
export const MIRRORED_DIRECTION: Record<Direction, Direction> = { ne: 'nw', se: 'sw', sw: 'se', nw: 'ne' };
export const OPPOSITE_DIRECTION: Record<Direction, Direction> = { ne: 'sw', se: 'nw', sw: 'ne', nw: 'se' };
export const WINDOW_ROTATION_BY_WALL: Record<WallSide, Rotation> = { west: 90, north: 180 };
export const wallRotation = (wall: WallSide): Rotation => WINDOW_ROTATION_BY_WALL[wall];

export function resolveTurnaroundView(spec: TurnaroundSpec, target: Direction): ResolvedSpriteView {
  const candidates = (Object.entries(spec.sources) as Array<[SpriteSource, Direction]>).flatMap(([source, facing]) => [
    { source, mirrorX: false, facing },
    { source, mirrorX: true, facing: MIRRORED_DIRECTION[facing] },
  ]);
  if (spec.orientation === 'invariant') {
    const first = candidates.find((candidate) => !candidate.mirrorX);
    if (first) return first;
  }
  const exact = candidates.find((candidate) => candidate.facing === target);
  if (exact) return exact;
  if (spec.orientation === 'axial') {
    const opposite = candidates.find((candidate) => candidate.facing === OPPOSITE_DIRECTION[target]);
    if (opposite) return opposite;
  }
  throw new Error(`no ${spec.orientation} sprite view can face ${target}`);
}

const spriteFiles = import.meta.glob<string>('../../assets/interior/furniture/**/*.png', { eager: true, query: '?url', import: 'default' });
type AssetMetric = { width: number; height: number; ground: number[] };
const assetMetrics = assetMetricsJson as Record<string, AssetMetric>;
const masks: Record<string, string[]> = {
  'sofa.mint-loveseat':['1111','1111'], 'sofa.coral-two-seat':['1111','1111'], 'sofa.lavender-sectional':['11111','11111','00111'], 'sofa.wood-frame':['1111','1111'], 'sofa.navy-modern':['1111','1111'],
  'lighting.warm-floor':['1'], 'lighting.pastel-table':['1'], 'lighting.retro-stand':['1'], 'lighting.paper-lantern':['1'], 'lighting.modern-arc':['10','11'],
  'desk.compact-study':['111','111'], 'desk.computer':['111','111'], 'desk.wood-writing':['111','111'], 'desk.pastel-vanity':['111','111'], 'desk.metal-workstation':['111','111'],
  'table.round-cafe':['11','11'], 'table.square-two':['11','11'], 'table.wood-four':['1111','1111'], 'table.pastel-breakfast':['11','11'], 'table.dark-modern':['11111','11111'],
  'chair.mint-cafe':['1'], 'chair.coral-compact':['1'], 'chair.warm-wood':['1'], 'chair.pastel-cream':['1'], 'chair.dark-modern':['1'],
  'appliance.retro-tv':['111','111'], 'appliance.compact-fridge':['11','11'], 'appliance.washer':['11','11'], 'appliance.stereo':['111','111'], 'appliance.desktop':['111','111'],
  'window.mint-square':['111'], 'window.cream-wood':['111'], 'window.coral-arch':['111'], 'window.lavender-bay':['11111'], 'window.navy-wide':['11111'],
};
const cellsFromMask = (id: string): FootprintCell[] => (masks[id] ?? ['1']).flatMap((row, y) => [...row].flatMap((value, x) => value === '1' ? [[x, y] as FootprintCell] : []));
const spriteUrl = (category: FurnitureCategory, id: string, direction: SpriteSource) => {
  const suffix = `/furniture/${category}/${id}/${direction}.png`;
  const entry = Object.entries(spriteFiles).find(([path]) => path.endsWith(suffix));
  if (!entry) throw new Error(`missing furniture sprite: ${category}/${id}/${direction}`);
  return entry[1];
};
const metricFor = (category: FurnitureCategory, id: string, direction: SpriteSource): AssetMetric => {
  const metric = assetMetrics[`${category}/${id}/${direction}`];
  if (!metric) throw new Error(`missing furniture metric: ${category}/${id}/${direction}`);
  return metric;
};
const directed = (ne: Direction, sw: Direction): TurnaroundSpec => ({ orientation: 'directed', sources: { ne, sw } });
const axial = (source: SpriteSource, facing: Direction): TurnaroundSpec => ({ orientation: 'axial', sources: { [source]: facing } });
const invariant = (source: SpriteSource): TurnaroundSpec => ({ orientation: 'invariant', sources: { [source]: 'ne' } });

// Source slots describe files, not their visual direction. These values were calibrated
// against the actual PNGs; the resolver derives the target view from this metadata.
const TURNAROUND_BY_ASSET: Record<string, TurnaroundSpec> = {
  'sofa.mint-loveseat': directed('se', 'nw'),
  'sofa.coral-two-seat': directed('se', 'nw'),
  'sofa.lavender-sectional': directed('se', 'nw'),
  'sofa.wood-frame': directed('se', 'nw'),
  'sofa.navy-modern': directed('se', 'nw'),

  'lighting.warm-floor': invariant('ne'),
  'lighting.pastel-table': invariant('ne'),
  'lighting.retro-stand': invariant('ne'),
  'lighting.paper-lantern': invariant('ne'),
  'lighting.modern-arc': axial('ne', 'nw'),

  'desk.compact-study': directed('se', 'ne'),
  'desk.computer': directed('se', 'ne'),
  'desk.wood-writing': directed('se', 'ne'),
  'desk.pastel-vanity': directed('se', 'ne'),
  'desk.metal-workstation': directed('se', 'ne'),

  'table.round-cafe': axial('sw', 'sw'),
  'table.square-two': axial('sw', 'sw'),
  'table.wood-four': axial('sw', 'sw'),
  'table.pastel-breakfast': axial('sw', 'sw'),
  'table.dark-modern': axial('sw', 'sw'),

  'chair.mint-cafe': directed('sw', 'ne'),
  'chair.coral-compact': directed('sw', 'ne'),
  'chair.warm-wood': directed('sw', 'ne'),
  'chair.pastel-cream': directed('sw', 'ne'),
  'chair.dark-modern': directed('sw', 'ne'),

  'appliance.retro-tv': directed('se', 'nw'),
  'appliance.compact-fridge': directed('se', 'nw'),
  'appliance.washer': directed('se', 'ne'),
  'appliance.stereo': directed('sw', 'ne'),
  'appliance.desktop': directed('sw', 'ne'),
};

const FOOTPRINT_ANCHOR_BY_ASSET: Partial<Record<string, [number, number]>> = {
  // The measured bottom pixel is the front edge of the centered 1x1 pedestal,
  // not the center or the front corner of the complete 2x2 table footprint.
  'table.round-cafe': [0.75, 0.75],
};

// Linear regressions of each source PNG's opaque bottom frame. Mirroring only
// changes the sign; a Y shear then makes the frame exactly parallel to the
// 2:1 isometric wall plane (ADR 0008).
export const WINDOW_SOURCE_EDGE_SLOPE_BY_ASSET: Record<string, number> = {
  'window.mint-square': -0.363812,
  'window.cream-wood': -0.35995,
  'window.coral-arch': 0.505936,
  'window.lavender-bay': 0.173727,
  'window.navy-wide': 0.100863,
};
export const WINDOW_TARGET_EDGE_SLOPE_BY_ROTATION: Partial<Record<Rotation, number>> = {
  90: -0.5,
  180: 0.5,
};

// Measured straight edges for floor sprites whose two isometric axes can be
// identified reliably. Organic and round assets intentionally keep the source
// projection instead of being distorted by a low-confidence measurement
  // (ADR 0015).
export const FLOOR_SOURCE_AXES_BY_ASSET: Partial<Record<string, Partial<Record<SpriteSource, [number, number]>>>> = {
  'sofa.mint-loveseat': { ne: [0.571, -0.559], sw: [0.560, -0.559] },
  'sofa.coral-two-seat': { ne: [0.560, -0.550], sw: [0.557, -0.569] },
  'sofa.lavender-sectional': { ne: [0.577, -0.554], sw: [0.551, -0.562] },
  'sofa.wood-frame': { ne: [0.560, -0.547], sw: [0.583, -0.550] },
  'sofa.navy-modern': { ne: [0.572, -0.543], sw: [0.576, -0.571] },

  // Desk axes use the visible outer tabletop edges. Internal drawer, monitor,
  // and support lines are deliberately excluded because generated sprites are
  // not internally parallel.
  'desk.compact-study': { ne: [0.580, -0.420], sw: [0.500, -0.489] },
  'desk.computer': { ne: [0.550, -0.400], sw: [0.500, -0.524] },
  'desk.wood-writing': { ne: [0.580, -0.415], sw: [0.468, -0.496] },
  'desk.pastel-vanity': { ne: [0.550, -0.420], sw: [0.470, -0.493] },
  'desk.metal-workstation': { ne: [0.590, -0.394], sw: [0.488, -0.492] },

  'table.square-two': { sw: [0.536, -0.559] },
  'table.wood-four': { sw: [0.538, -0.576] },
  'table.pastel-breakfast': { sw: [0.545, -0.548] },
  'table.dark-modern': { sw: [0.545, -0.555] },

  'appliance.retro-tv': { ne: [0.536, -0.500], sw: [0.529, -0.525] },
  'appliance.compact-fridge': { ne: [0.500, -0.538], sw: [0.511, -0.547] },
  'appliance.washer': { ne: [0.520, -0.522], sw: [0.535, -0.529] },
  'appliance.stereo': { ne: [0.500, -0.480], sw: [0.525, -0.557] },
  'appliance.desktop': { ne: [0.515, -0.504], sw: [0.522, -0.522] },
};

export const FLOOR_PROJECTION_EXEMPT_BY_ASSET: Record<string, string> = {
  'lighting.warm-floor': 'no stable pair of straight floor-plane edges',
  'lighting.pastel-table': 'no stable pair of straight floor-plane edges',
  'lighting.retro-stand': 'no stable pair of straight floor-plane edges',
  'lighting.paper-lantern': 'rotational silhouette',
  'lighting.modern-arc': 'curved silhouette',
  'table.round-cafe': 'round tabletop',
  'chair.mint-cafe': 'one-cell organic silhouette',
  'chair.coral-compact': 'one-cell organic silhouette',
  'chair.warm-wood': 'one-cell organic silhouette',
  'chair.pastel-cream': 'one-cell organic silhouette',
  'chair.dark-modern': 'one-cell organic silhouette',
};

// Visible floor-support contour for each independent desk source PNG. These are
// pixel coordinates, not per-direction offsets. Mirrored views reuse the same
// contacts with their X coordinates reflected (ADR 0011).
export const DESK_SUPPORT_CONTACTS_BY_ASSET: Record<string, Record<SpriteSource, FootprintCell[]>> = {
  'desk.compact-study': {
    sw: [[18,243],[163,319],[167,319],[259,275],[262,273]],
    ne: [[17,304],[83,340],[88,340],[191,306],[258,281],[262,279]],
  },
  'desk.computer': {
    sw: [[13,234],[58,257],[170,307],[175,307],[252,270],[255,268]],
    ne: [[13,270],[18,273],[87,311],[90,311],[204,275],[256,255],[259,252]],
  },
  'desk.wood-writing': {
    sw: [[13,208],[16,210],[180,282],[185,282],[257,249],[266,244]],
    ne: [[11,234],[13,236],[86,276],[92,276],[263,208],[271,204]],
  },
  'desk.pastel-vanity': {
    sw: [[16,273],[69,250],[149,334],[154,334],[205,309],[210,309]],
    ne: [[17,327],[62,354],[67,354],[162,273],[206,296],[211,296]],
  },
  'desk.metal-workstation': {
    sw: [[12,243],[111,291],[168,320],[171,321],[176,321],[245,287],[253,283]],
    ne: [[9,270],[13,272],[80,312],[85,312],[185,275],[254,251],[260,247]],
  },
};
export const DESK_SUPPORT_PADDING = 0.02;

type DeskRegistration = { widthTiles: number; anchor: [number, number] };

function deskRegistration(
  assetId: string,
  size: [number, number],
  rotation: Rotation,
  metric: AssetMetric,
  source: SpriteSource,
  mirrorX: boolean,
  projection: SpriteProjection,
  sharedWidthTiles?: number,
): DeskRegistration {
  const contacts = DESK_SUPPORT_CONTACTS_BY_ASSET[assetId]?.[source];
  if (!contacts?.length) throw new Error(`missing desk support contacts: ${assetId}/${source}`);
  const [width, height] = rotation === 90 || rotation === 270 ? [size[1], size[0]] : size;
  const worldContacts = contacts.map(([sourceX, sourceY]) => {
    const x = mirrorX ? metric.width - sourceX : sourceX;
    const y = projection.shearY * x + projection.scaleY * sourceY;
    return [(x + 2 * y) / 2, (2 * y - x) / 2] as const;
  });
  const u = worldContacts.map(([value]) => value);
  const v = worldContacts.map(([, value]) => value);
  const minU = Math.min(...u), maxU = Math.max(...u);
  const minV = Math.min(...v), maxV = Math.max(...v);
  const maximumScale = Math.min(
    (width - 2 * DESK_SUPPORT_PADDING) / (maxU - minU),
    (height - 2 * DESK_SUPPORT_PADDING) / (maxV - minV),
  );
  const scale = sharedWidthTiles === undefined
    ? maximumScale
    : Math.min(maximumScale, 2 * sharedWidthTiles / metric.width);
  // The visible supports sit against the front of the occupied cells. Any
  // unused depth remains behind the desk instead of making it float midway
  // through a long 2x4 footprint.
  const translateU = -DESK_SUPPORT_PADDING - scale * maxU;
  const translateV = -DESK_SUPPORT_PADDING - scale * maxV;
  const anchorX = (translateV - translateU) / scale;
  const anchorY = -(translateU + translateV) / (2 * scale);
  return {
    widthTiles: metric.width * scale / 2,
    anchor: [anchorX / metric.width, anchorY / metric.height],
  };
}

const turnaroundFor = (assetId: string, category: FurnitureCategory): TurnaroundSpec | undefined => {
  if (category === 'window') return undefined;
  const spec = TURNAROUND_BY_ASSET[assetId];
  if (!spec) throw new Error(`missing turnaround metadata: ${assetId}`);
  return spec;
};
const make = (category: FurnitureCategory, rows: Array<[string, string, [number, number], number?]>): FurnitureItem[] =>
  rows.map(([id, name, size, widthTiles]) => {
    const assetId = `${category}.${id}`;
    const turnaround = turnaroundFor(assetId, category);
    const windowSourceSlope = () => {
      const slope = WINDOW_SOURCE_EDGE_SLOPE_BY_ASSET[assetId];
      if (category === 'window' && slope === undefined) throw new Error(`missing window slope metadata: ${assetId}`);
      return slope ?? 0;
    };
    const viewFor = (rotation: Rotation): ResolvedSpriteView => {
      if (turnaround) return resolveTurnaroundView(turnaround, SPRITE_DIRECTION_BY_ROTATION[rotation]);
      const sourceSlope = windowSourceSlope();
      const targetSlope = WINDOW_TARGET_EDGE_SLOPE_BY_ROTATION[rotation];
      const mirrorX = targetSlope === undefined ? false : Math.sign(sourceSlope) !== Math.sign(targetSlope);
      return { source: 'ne', mirrorX, facing: SPRITE_DIRECTION_BY_ROTATION[rotation] };
    };
    const projectionFor = (rotation: Rotation): SpriteProjection => {
      const view = viewFor(rotation);
      if (category === 'window') {
        const targetSlope = WINDOW_TARGET_EDGE_SLOPE_BY_ROTATION[rotation];
        if (targetSlope === undefined) return { scaleY: 1, shearY: 0 };
        const sourceSlope = windowSourceSlope();
        const effectiveSlope = view.mirrorX ? -sourceSlope : sourceSlope;
        return { scaleY: 1, shearY: targetSlope - effectiveSlope };
      }
      const axes = FLOOR_SOURCE_AXES_BY_ASSET[assetId]?.[view.source];
      if (axes) {
        const [sourcePositive, sourceNegative] = axes;
        const effectivePositive = view.mirrorX ? -sourceNegative : sourcePositive;
        const effectiveNegative = view.mirrorX ? -sourcePositive : sourceNegative;
        const scaleY = 1 / (effectivePositive - effectiveNegative);
        return { scaleY, shearY: 0.5 - scaleY * effectivePositive };
      }
      return { scaleY: 1, shearY: 0 };
    };
    const maximumDeskRegistrationFor = (rotation: Rotation): DeskRegistration | undefined => {
      if (category !== 'desk') return undefined;
      const view = viewFor(rotation);
      return deskRegistration(assetId, size, rotation, metricFor(category, id, view.source), view.source, view.mirrorX, projectionFor(rotation));
    };
    const sharedDeskWidthTiles = category === 'desk'
      ? Math.min(...ROTATIONS.map((rotation) => maximumDeskRegistrationFor(rotation)!.widthTiles))
      : undefined;
    const deskRegistrationFor = (rotation: Rotation): DeskRegistration | undefined => {
      if (category !== 'desk') return undefined;
      const view = viewFor(rotation);
      return deskRegistration(assetId, size, rotation, metricFor(category, id, view.source), view.source, view.mirrorX, projectionFor(rotation), sharedDeskWidthTiles);
    };
    const anchorFor = (rotation: Rotation): [number, number] => {
      const { source, mirrorX } = viewFor(rotation);
      const metric = metricFor(category, id, source);
      if (category === 'window') {
        const centerSourceY = metric.ground[1]
          + windowSourceSlope() * (0.5 - metric.ground[0]) * metric.width / metric.height;
        const projection = projectionFor(rotation);
        const centerTransformedY = projection.scaleY * centerSourceY
          + projection.shearY * 0.5 * metric.width / metric.height;
        return [0.5, centerTransformedY];
      }
      const registration = deskRegistrationFor(rotation);
      if (registration) return registration.anchor;
      const measuredImageX = mirrorX ? 1 - metric.ground[0] : metric.ground[0];
      const projection = projectionFor(rotation);
      const transformedY = projection.scaleY * metric.ground[1] + projection.shearY * measuredImageX * metric.width / metric.height;
      return [measuredImageX, transformedY];
    };
    const baseWidthTiles = widthTiles ?? (size[0] + size[1]) / 2;
    return {
      id: assetId, category, name, size, footprint: cellsFromMask(assetId), turnaround,
      sprites: Object.fromEntries(ROTATIONS.map((rotation) => [rotation, spriteUrl(category, id, viewFor(rotation).source)])) as Record<Rotation, string>,
      render: {
        widthTiles: Object.fromEntries(ROTATIONS.map((rotation) => [rotation, deskRegistrationFor(rotation)?.widthTiles ?? baseWidthTiles])) as Record<Rotation, number>,
        anchors: Object.fromEntries(ROTATIONS.map((rotation) => [rotation, anchorFor(rotation)])) as Record<Rotation, [number, number]>,
        footprintAnchor: FOOTPRINT_ANCHOR_BY_ASSET[assetId] ?? [1, 1],
        mirrorX: Object.fromEntries(ROTATIONS.map((rotation) => [rotation, viewFor(rotation).mirrorX])) as Record<Rotation, boolean>,
        projections: Object.fromEntries(ROTATIONS.map((rotation) => [rotation, projectionFor(rotation)])) as Record<Rotation, SpriteProjection>,
        sources: Object.fromEntries(ROTATIONS.map((rotation) => [rotation, viewFor(rotation).source])) as Record<Rotation, SpriteSource>,
      },
    };
  });

export const FURNITURE: FurnitureItem[] = [
  ...make('sofa', [['mint-loveseat','민트 러브시트',[4,2]],['coral-two-seat','코랄 2인 소파',[4,2]],['lavender-sectional','라벤더 코너 소파',[5,3]],['wood-frame','우드 프레임 소파',[4,2]],['navy-modern','네이비 모던 소파',[4,2]]]),
  ...make('lighting', [['warm-floor','웜 플로어 램프',[1,1],0.82],['pastel-table','파스텔 테이블 램프',[1,1],0.72],['retro-stand','레트로 스탠드',[1,1],0.9],['paper-lantern','페이퍼 랜턴',[1,1],0.82],['modern-arc','모던 아크 램프',[2,2],1.8]]),
  ...make('desk', [['compact-study','콤팩트 공부책상',[3,2]],['computer','컴퓨터 책상',[3,2]],['wood-writing','우드 집필책상',[3,2]],['pastel-vanity','파스텔 화장대',[3,2]],['metal-workstation','메탈 워크스테이션',[3,2]]]),
  ...make('table', [['round-cafe','민트 원형 테이블',[2,2]],['square-two','코랄 사각 테이블',[2,2]],['wood-four','우드 다이닝 테이블',[4,2]],['pastel-breakfast','파스텔 테이블',[2,2]],['dark-modern','다크 모던 테이블',[5,2]]]),
  ...make('chair', [['mint-cafe','민트 카페 의자',[1,1],1.05],['coral-compact','코랄 의자',[1,1],1.05],['warm-wood','우드 의자',[1,1],1.05],['pastel-cream','파스텔 의자',[1,1],1.05],['dark-modern','다크 모던 의자',[1,1],1.05]]),
  ...make('appliance', [['retro-tv','레트로 TV',[3,2]],['compact-fridge','콤팩트 냉장고',[2,2]],['washer','세탁기',[2,2]],['stereo','오디오 장식장',[3,2]],['desktop','데스크톱 세트',[3,2]]]),
  ...make('window', [['mint-square','민트 사각창',[3,2]],['cream-wood','크림 우드창',[3,2]],['coral-arch','코랄 아치창',[3,3]],['lavender-bay','라벤더 베이창',[5,3]],['navy-wide','네이비 와이드창',[5,2]]]),
];
export const FURNITURE_BY_ID = new Map(FURNITURE.map((item) => [item.id, item]));

export function canonicalizeFurnitureGeometry<T extends PlacedFurniture>(object: T): T {
  const item = FURNITURE_BY_ID.get(object.asset_id);
  if (!item) return object;
  return {
    ...object,
    category: item.category,
    size: [...item.size] as [number, number],
    footprint: item.footprint.map((cell) => [...cell] as FootprintCell),
  } as T;
}

export const THEMES: InteriorTheme[] = [
  ['lavender-dream','라벤더 드림','#e8e1f4','#d8cce9','#eadbc6','#dfc9ad','#c6ae90'],
  ['mint-cafe','민트 카페','#dceee5','#c9e1d5','#f1e3bd','#e9d6a7','#c7b588'],
  ['coral-sunset','코랄 선셋','#f5d2c7','#e9bfb3','#e4b0a5','#d79b91','#bd8279'],
  ['blue-night','블루 나이트','#7182a7','#5d6e94','#596a88','#4d5d79','#394863'],
  ['retro-pop','레트로 팝','#ffd783','#f0b962','#e9a865','#d8904d','#ba7137'],
  ['forest-cabin','포레스트 그린','#cbdcbc','#b5cba5','#9b7657','#866247','#654631'],
  ['mono-studio','모노 스튜디오','#e7e7e4','#d4d4d0','#c7c7c3','#b8b8b4','#999995'],
  ['peach-bedlife','피치 베드라이프','#f6d9ca','#eac4b2','#f1e7d7','#e4d5c1','#c9b79e'],
  ['cyber-life','사이버 라이프','#4b3e6d','#382d59','#313d56','#263148','#38b7ad'],
  ['sky-loft','스카이 블루','#cce9f2','#b4d8e5','#dbc69f','#ceb78c','#aa9168'],
  ['special-loft','복층 로프트','#c8a889','#b49172','#d8c09e','#c6a982','#967653'],
  ['special-clinic','동네 병원','#d8eee8','#c3dfd7','#efe4cc','#e4d5b8','#aa9a7d'],
  ['special-rooftop','옥상 카페','#a8d7dc','#8bc1c8','#c7ad86','#b7976e','#78664f'],
  ['special-sky-loft','스카이 로프트','#cce9f2','#b4d8e5','#dbc69f','#ceb78c','#aa9168'],
  ['special-forest-cabin','포레스트 캐빈','#cbdcbc','#b5cba5','#9b7657','#866247','#654631'],
].map(([id,name,wall,wallSide,floorBase,floorAlt,grout]) => ({ id, name, wallpaper:id, floor:`${id}-floor`, wall, wallSide, floorBase, floorAlt, grout })) as InteriorTheme[];

export function themeFor(wallpaper: string, floor: string): InteriorTheme { return THEMES.find((t) => t.wallpaper === wallpaper && t.floor === floor) ?? THEMES[0]; }
