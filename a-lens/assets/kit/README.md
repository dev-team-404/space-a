# 스프라이트 킷 — 방 에셋 제작 가이드

방 씬은 **완성된 방 배경 프리셋 1장 + 그 위에 얹는 책상**으로 그려진다.
이 폴더에 PNG를 넣고 `manifest.json`에 등록하면 frontend 렌더러가 자동으로 로드한다.
등록 안 되거나 못 뜬 부품은 도형(Graphics)으로 폴백한다.

레퍼런스 아트: 리포 루트 `room.png` (아이소메트릭 픽셀아트, 야간 우드톤).

## 지금 쓰는 부품

| 키 | 파일 | 내용 |
|---|---|---|
| `room.r1`~`room.r5` | `room-preset-1~5.png` | **방 배경 프리셋 5종** — 벽·바닥·책장·칠판이 다 그려진 팔각 방 통짜 이미지. `cal`(좌우 바닥 꼭짓점·기울기)로 2:1 격자에 정규화되고, 그 위에 책상만 얹힌다. 이미지의 칠판·책장 영역(`cal.backWall`/`cal.shelfArea`)에는 투명 히트존이 깔려 이슈/지식 패널을 연다. |
| `shell.room` | `shell-room.png` | 프리셋 로드 실패 시 폴백용 빈 방 셸 1장. |
| `desk.d1`~`desk.d9` | `desk-d1~9.png` | 배경 위에 얹는 책상 세트 9종 (모니터·의자·소품 포함). footprint `[2, 1]`. |

## 공통 규격

| 항목 | 값 |
|---|---|
| 프로젝션 | 2:1 아이소메트릭(다이메트릭 26.57°), 남쪽(정면 아래)에서 본 시점 |
| 논리 타일 | 가로 64px × 세로 32px (다이아몬드 1칸) |
| 에셋 해상도 | **4배** — 타일 1칸 = 256×128px (manifest `scale: 0.25`로 축소 렌더) |
| 조명 | 좌상단 따뜻한 전구빛 (room.png와 동일 톤) |

**앵커 규칙**
- 책상(`mount: "floor"`): 투명 PNG, **이미지 하단 중앙** = footprint 다이아몬드의 남쪽(앞) 꼭짓점.
  바닥·벽 포함 금지, 부드러운 접지 그림자만 허용. 트림 후 미세 오차는 manifest `offset`으로 보정.
- 방 배경(`mount: "shell-room"`): 팔각 방 전체를 그린 한 장. 투명 배경 위 방. `cal`로 격자에 맞춘다.

## 방 배경 프리셋 `cal` 재기

새 배경 이미지를 넣을 때 `cal`을 다시 잰다 (이미지 픽셀 좌표 기준):

| 필드 | 뜻 |
|---|---|
| `left` / `right` | 팔각 바닥의 좌·우 극점 (같은 y) |
| `slope` | 바닥 가장자리 아이소 기울기 (2:1이면 `0.5`) |
| `backEdgeY` | 뒷벽 밑 바닥 시작 y — 책상이 벽 뒤로 넘어가지 않게 하는 컷 |
| `backWall` | 칠판 영역 `[x0,y0,x1,y1]` — 클릭 시 이슈 패널 |
| `shelfArea` | 왼벽 책장 영역 `[x0,y0,x1,y1]` — 클릭 시 지식 패널 (선택) |

5장이 같은 구도면 `cal`을 공유해도 된다 (현재 프리셋 5종이 그렇다).

## 생성 프롬프트 템플릿

책상 예시 (variant는 재질 단어만 교체 — `walnut dark wood`, `clean white modern` 등):

> isometric pixel art of a single wooden office desk set — two dark monitors, a laptop,
> a coffee mug, an office chair in front — 2:1 dimetric projection (26.57°), front-south view,
> warm night interior lighting from upper-left string bulbs, cozy dark-academia palette with
> warm brown wood (#8a5f36 tones), **transparent background, object only, no floor, no wall**,
> soft contact shadow only, crisp pixel-art edges, single object centered

## 등록 방법

1. PNG를 이 폴더에 넣는다.
2. `manifest.json`의 `pieces`에 추가:

```json
"desk.d10": { "file": "desk-d10.png", "footprint": [2, 1] }
```

3. 브라우저 새로고침 — 끝. 위치가 어긋나면 `"offset": [x, y]`(에셋 px 기준)로 보정한다.
