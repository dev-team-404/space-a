# A-Hub Life (space-a-room-server)

방 방문(Room Visit) 서버 — 개인 방·에이전트 위치·방 디자인.
**A-Hub Work와 별개의 서버 프로세스**입니다. 같은 `a-hub/` 아래에 있지만 Python 프로젝트,
Docker Compose, 데이터베이스와 실행 생애주기를 공유하지 않습니다.
설계: [docs/design/room-visit.md](../../docs/design/room-visit.md)

- 유저당 방 1개 (20×20 아이소메트릭 바닥 셀), 에이전트 위치의 단일 원천
- room protocol v3: 가구별 footprint와 벽에서 파생되는 창문 방향(`west=90`, `north=180`)
- 겹침 금지: "빈 셀일 때만 점유"를 전역 락 안에서 원자 처리 (`409 cell_taken`)
- 영속화: `ROOM_SERVER_DB` 설정 시 SQLite(볼륨)로 등록·방·위치·디자인 유지
  (compose 기본 활성). 미설정이면 인메모리 — 재시작 시 초기화

## 실행

```bash
cd a-hub/life
docker compose up -d --build   # 호스트 포트 8001
```

클라이언트(Agent Mentor) 설정의 "Space A 서버" URL에 `http://localhost:8001`을 입력합니다.

## 사외 배포 (현재 운영 중)

room_server는 인메모리 + SQLite 단일 프로세스라 **상주 서버**로 운영합니다. Oracle Cloud
상시 무료 VM에 Docker로 띄워 두었습니다. 배포·운영 상세: [DEPLOY.md](./DEPLOY.md).

| 항목 | 값 |
|---|---|
| Base URL | `http://158.179.194.42:8001` |
| OpenAPI 문서 | `http://158.179.194.42:8001/docs` |
| 인증 | `x-api-key` 헤더 (관문 활성) + 엔드포인트별 `Authorization: Bearer <token>` |
| VM | OCI Ubuntu 24.04 (AMD, x86_64), Docker Compose, `restart: unless-stopped` |
| 영속 | SQLite 볼륨(`room-server-data`) — 재시작·재부팅에도 방·위치·디자인 유지 |

> ⚠️ 평문 HTTP라 토큰·`x-api-key`가 네트워크에 그대로 흐릅니다. **테스트 용도로만** 쓰고,
> 운영이 필요해지면 HTTPS(Caddy/Cloudflare Tunnel)를 앞에 두세요.

```bash
API=http://158.179.194.42:8001
KEY=<x-api-key>                                   # 배포 시 정한 ROOM_SERVER_API_KEY

curl $API/healthz                                 # 관문 면제 → 200 (키 없이도)
curl $API/ -H "x-api-key: $KEY"                   # discovery (api_key_required: true)
curl -X POST $API/rooms/register -H "x-api-key: $KEY" \
  -H 'content-type: application/json' -d '{"name":"bot"}'
```

### x-api-key 관문

`ROOM_SERVER_API_KEY` 환경변수를 주면, `/healthz`·`/readyz`를 제외한 모든 요청이
`x-api-key: <고정키>` 헤더를 요구합니다. 비우면(기본) 관문 비활성 — 로컬 실행 시 편의.
평문 HTTP로 노출하는 테스트 배포에서 최소한의 접근 통제 용도입니다.

### 로컬에서 동일 구성으로 실행

```bash
cd a-hub/life
ROOM_SERVER_API_KEY=<고정키> docker compose up -d --build   # → http://localhost:8001
```

## 테스트

```bash
pip install -e ".[dev]"
pytest
```

## API

| Method | Path | 동작 |
|---|---|---|
| POST | `/rooms/register` | 유저 등록 + 개인 방 생성 (자기 방 자동 입장) |
| GET | `/rooms` | 방 목록 |
| GET | `/rooms/me` | 내 위치 |
| PATCH | `/rooms/me` | 이름 변경 (에이전트 + 내 방 주인 이름) |
| GET | `/rooms/{id}` | 방 상태 (디자인 + 입장자) |
| POST | `/rooms/{id}/enter` | 입장 (cell 생략 시 서버가 빈 셀 배정) |
| POST | `/rooms/{id}/move` | 방 안 셀 이동 |
| PUT | `/rooms/{id}/design` | 방 디자인 변경 (주인만, 다중 셀 footprint·4방향 회전 포함) |
