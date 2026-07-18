# ADR 0003: 방 배치 지오메트리 v2

- 상태: 채택
- 날짜: 2026-07-18

## 배경

기존 인테리어 구현은 `cell`을 클릭 셀, 직사각형 좌상단, 이미지 표시 중심으로 혼용했다.
클라이언트와 room-server가 격자 크기와 바닥 범위를 중복 정의했고, 에셋에는 명시적인
점유 셀과 ground anchor가 없었다. 그 결과 가장자리 배치, 회전, 비정형 가구, 구버전
서버 저장에서 반복적인 좌표 오류가 발생했다.

## 결정

### 단일 월드 모델

- 방 크기는 프로토콜 v2에서 `20×20`이다.
- 배치 객체의 `cell`은 항상 **현재 회전이 적용된 footprint 바운딩 박스의 월드 원점**이다.
- 사용자가 클릭한 셀은 `targetCell`이며 저장하지 않는다. 순수 함수
  `placementFromTarget(asset, rotation, targetCell)`이 원점을 계산하고 방 안으로 보정한다.
- 점유는 직사각형 크기가 아니라 에셋별 `footprint: [[dx,dy], ...]`로 계산한다.
- 회전은 스프라이트 방향, footprint 셀, 바운딩 크기에 동시에 적용한다.

### 에셋 계약

- `asset_id/방향.png` 독립 투명 PNG만 사용한다. 런타임 아틀라스 분할은 금지한다.
- 각 PNG의 ground anchor는 파일 하단 중앙으로 정규화한다.
- 카탈로그는 `asset_id`, 카테고리, 기본 크기, footprint, 네 방향 파일을 가진다.

### 투영 계약

```text
screenX = originX + (worldX - worldY) * tileWidth / 2
screenY = originY + (worldX + worldY) * tileHeight / 2
```

- 바닥 가구의 이미지 ground anchor는 회전된 footprint 바운딩 박스 중심의 투영점이다.
- 벽 가구는 `wall=north|west`와 벽 축의 원점을 사용하며 바닥 좌표와 섞지 않는다.
- 렌더러는 월드 좌표를 수정하지 않고 geometry 모듈 결과만 표시한다.

### 서버 계약

- `/capabilities`는 `room_protocol: 2`, `grid: {w:20,h:20}`, `floor_min_y: 0`을 반환한다.
- 클라이언트는 편집 시작과 저장 전에 프로토콜을 확인한다.
- 구버전 서버에는 v2 디자인을 전송하지 않고 재시작 필요 오류를 표시한다.
- SQLite는 footprint와 wall을 저장하며, footprint가 없는 v1 객체만 직사각형으로 복원한다.

## 검증

- 네 모서리 targetCell에서 대형 가구가 방 안으로 보정된다.
- 비정형 footprint의 빈 셀에는 다른 가구를 맞물려 배치할 수 있다.
- 4방향 회전 후 점유 셀과 이미지 방향이 일치한다.
- 저장 후 재조회와 서버 재시작 후에도 cell, rotation, footprint, wall이 동일하다.
