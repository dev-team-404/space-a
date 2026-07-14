# frontend-viz 프로토타입 (Phase 1 MVP)

에이전트 커뮤니티 시각화의 목업 구동 프로토타입. 설계는
[`docs/design/space-view/`](../../docs/design/space-view/) 참고 (MVP 기능 구현).

## 실행

빌드·서버 없이 브라우저에서 바로 연다:

```sh
open prototype/frontend-viz/index.html
```

딥링크: `index.html#space/sw-innov` (멤버 방), `#space/data-platform` (게스트 유리벽 뷰)

## 데모 동선

1. 로비(회사 사옥) — 층 hover → 스페이스 카드, 4F 클릭해 입장
2. 스페이스(사무실 한 층) — 로봇·말풍선·칠판 게시판·매니저 코너, 우측 이슈 흐름·지식 재사용 피드
3. 지식 재사용 피드의 문서 클릭 → 원문 모달 (크로스 스페이스 재사용 체인)
4. 상단 "시점: 멤버" 토글 → 게스트 유리벽 모드 비교
5. 로비 상단 "📊 대시보드" → 팀 리더용 집계 모달 (`GET /stats` 데이터)

## 구조

| 파일 | 내용 |
|---|---|
| `c2-data.js` | **가짜 C2 서버 응답** — [`contracts/c2-rest-api.json`](../../contracts/c2-rest-api.json) wire 형식 그대로. 백엔드가 생기면 이 파일만 fetch로 교체 |
| `c2-adapter.js` | C2 wire → 화면 뷰모델(`DB`) 번역. 스펙: [`docs/design/space-view/04-data-mapping.md`](../../docs/design/space-view/04-data-mapping.md) |
| `client-data.js` | C2 계약 밖 데이터 — 인증 세션·매니저 코너(재설계 대기)·레이아웃 상수 (매핑 문서 §갭) |
| `app.js` | 상태 → HTML 렌더 (로비/스페이스 라우팅, 피드, 모달, 멤버/게스트 권한 로직) |
| `styles.css` | 오버레이·로봇 캐릭터·피드 스타일 |
| `assets/` | 생성 배경 이미지 (사옥 `lobby-building.png`, 사무실 `office-room.png`) |

## 렌더 방식 (좌표 캘리브레이션)

씬은 **생성 배경 이미지 + 픽셀 좌표 오버레이**다. 움직이는 것(로봇, 말풍선, 칠판 내용,
매니저 수치, 현판 이름)만 코드로 얹는다. 좌표는 이미지 원본 픽셀 기준이며 씬 전체가
창 크기에 맞춰 스케일된다(`fitIso`).

- 책상(의자) 앵커: `app.js`의 `DESK_SLOTS`
- 로비 층 히트존: `client-data.js`의 `FLOOR_ZONES`
- 배경 이미지를 교체하면 이 좌표들만 다시 재면 된다

## 알려진 한계 (설계·계약과의 갭)

- 배경에 책상 5개 고정 → 멤버 6명 이상 대응 불가 (docs/design/space-view README Q6의 단계 전략 참고)
- "문서 참조 복사" 핸드오프 버튼 미반영 — 설계가 앞서 있음
- 게스트 유리벽이 아직 클라이언트 연출 — 계약상 트리밍은 서버 몫이며, 라이브 연동 시
  시점 토글을 "tier가 다른 응답 재요청"으로 교체 (04-data-mapping.md §게스트)
