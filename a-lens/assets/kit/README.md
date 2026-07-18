# 스프라이트 킷 — 방 부품 에셋 제작 가이드

방 씬은 **격자 + 부품 데이터**로 그려진다. 이 폴더에 부품 PNG를 넣고 `manifest.json`에
등록하면, frontend 렌더러가 해당 부품을 도형(Graphics) 대신 **그 이미지(Sprite)로 자동 교체**한다.
등록 안 된 부품은 지금처럼 도형으로 폴백하므로, **한 부품씩 점진적으로** 올리면 된다.

레퍼런스 아트: 리포 루트 `room.png` (아이소메트릭 픽셀아트, 야간 우드톤).
가림(occlusion) 없는 부품은 room.png에서 잘라내(배경 투명화) 써도 된다.

## 공통 규격

| 항목 | 값 |
|---|---|
| 프로젝션 | 2:1 아이소메트릭(다이메트릭 26.57°), 남쪽(정면 아래)에서 본 시점 |
| 논리 타일 | 가로 64px × 세로 32px (다이아몬드 1칸) |
| 에셋 해상도 | **4배** — 타일 1칸 = 256×128px (manifest `scale: 0.25`로 축소 렌더) |
| 배경 | **투명 PNG**. 바닥·벽 포함 금지, 부드러운 접지 그림자만 허용 |
| 조명 | 좌상단 따뜻한 전구빛 (room.png와 동일 톤) |
| 트림 | 생성 후 투명 여백을 딱 맞게 잘라낼 것 (미세 오차는 manifest `offset`으로 보정) |

**앵커 규칙** — 렌더러가 이미지를 놓는 기준점:
- `mount: "floor"`(바닥 부품): **이미지 하단 중앙** = footprint 다이아몬드의 **남쪽(앞) 꼭짓점**
- `mount: "wall-right"`(오른쪽 뒷벽 부착물): **이미지 좌측 하단** = 벽면 부착 시작점.
  벽면은 오른쪽 아래로 2:1 기울기이므로, 부착물의 상·하 변도 같은 기울기로 그려져 있어야 한다.

## 부품 키와 권장 크기 (4배 해상도 기준)

| 키 | 내용 | footprint (칸) | 대략 캔버스 |
|---|---|---|---|
| `desk.oak` / `desk.walnut` / `desk.white` | 책상 세트 (모니터·의자·소품 포함) | [2, 1] | ~480×480 |
| `shelf.default` | 책장 (책 꽂힌 상태) | [0.7, 2.4] | ~440×640 |
| `board.chalk-green` / `board.chalk-black` / `board.glass` | 칠판/보드 (벽 원근, 면은 비워둘 것 — 이슈 텍스트를 코드가 얹음) | wall-right, 벽 4.6칸 | ~640×520 |
| `deco.plant` | 화분 | [0.55, 0.55] | ~200×280 |
| `deco.water-cooler` | 정수기 | [0.6, 0.6] | ~200×320 |
| `deco.rug` | 러그 (납작) | [2.8, 2.8] | ~720×370 |

> 로봇 스킨(`robot.*`)과 벽·바닥 텍스처는 다음 단계 — 렌더러 연결 시 여기 규격을 추가한다.

## 생성 프롬프트 템플릿

책상 예시 (다른 부품은 대상만 바꿔서):

> isometric pixel art of a single wooden office desk set — two dark monitors, a laptop,
> a coffee mug, an office chair in front — 2:1 dimetric projection (26.57°), front-south view,
> warm night interior lighting from upper-left string bulbs, cozy dark-academia palette with
> warm brown wood (#8a5f36 tones), **transparent background, object only, no floor, no wall**,
> soft contact shadow only, crisp pixel-art edges, single object centered

variant별로는 재질 단어만 교체: `walnut dark wood`, `clean white modern` 등.
칠판(wall-right)은 다음을 추가:

> the board is mounted on the right-side back wall of an isometric room, so the board's top and
> bottom edges slope down-to-the-right at a 2:1 isometric angle; leave the board face empty

## 등록 방법

1. PNG를 이 폴더에 넣는다 (예: `desk-oak.png`)
2. `manifest.json`의 `pieces`에 추가:

```json
"desk.oak": { "file": "desk-oak.png", "footprint": [2, 1] }
```

3. 브라우저 새로고침 — 끝. 위치가 어긋나면 `"offset": [x, y]`(에셋 px 기준)로 보정한다.
