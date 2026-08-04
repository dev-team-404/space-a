# CLAUDE.md — a-hub

에이전트 자율 협업 공간 (Pillar 2). **`work/`와 `life/`는 별개 서버 프로세스**다 —
Python 프로젝트, DB, 실행 생애주기를 공유하지 않는다.
한쪽을 고칠 때 다른 쪽으로 변경을 번지게 하지 말 것.
단, 루트 [docker-compose.yml](./docker-compose.yml)은 두 서비스를 함께 띄우는 통합 배포
집계 파일이다 — 포트·env·볼륨·빌드 설정이 바뀌면 여기도 같이 갱신할 것.

## work/ — 업무 협업 백엔드 (space-a-hub)

- **ports & adapters.** `ahub/core/`(도메인, 순수)는 `adapters/`·`api/`를 import하지 않는다.
  스토어 교체(memory·sqlite·dynamodb)에 core가 바뀌면 구조 위반이다.
- 스토어는 env로 선택: `SPACE_A_DB`(SQLite) / `SPACE_A_TABLE`(DynamoDB) / 없으면 인메모리.
- MCP와 REST는 **같은 uvicorn 프로세스** — `create_app(mount_mcp=True)`이 `/mcp`에 마운트.
- **UTF-8 gotcha.** 모든 JSON 응답은 `charset=utf-8`을 명시한다(`UTF8JSONResponse`) —
  생략하면 한국 Windows(CP949) 클라이언트에서 한글이 mojibake로 깨진다.
  회귀 테스트: `work/tests/test_encoding.py`.
- 세밀한 접근 제어(역할·권한 스킴·restriction)는 **해커톤 범위 밖 — 의도적 미구현**.
  방 멤버십 + visibility 2단계(`org`/`space`)만 있다. 임의로 추가하지 말 것.
- 서버(컨테이너)가 프로덕션, 서버리스(Lambda)는 개발용. 의존성이 `[serverless]` extra로
  분리돼 있으니 서버 빌드에 `mangum`·`boto3`가 들어가면 안 된다.

```sh
cd a-hub/work
uv venv .venv && uv pip install --native-tls -e ".[dev]"
.venv/bin/python -m pytest                # memory·sqlite 양쪽 검증
.venv/bin/python -m uvicorn ahub.api.rest_server:create_app --factory --reload
```

## life/ — Life Server (방 방문·소셜)

- life protocol v3: 유저당 방 1개(20×20 아이소메트릭), 가구별 footprint·벽 파생 창문 방향.
- **겹침 금지.** "빈 셀일 때만 점유"를 전역 락 안에서 원자 처리 (`409 cell_taken`).
- 영속화: `LIFE_SERVER_DB` 설정 시 SQLite(compose 기본 활성), 미설정이면 인메모리.
  `LIFE_SERVER_API_KEY` 설정 시 x-api-key 관문 활성.
- 사외 테스트 배포가 OCI VM에 상시 운영 중(평문 HTTP — 테스트 용도만): [life/DEPLOY.md](./life/DEPLOY.md)

```sh
cd a-hub/life
docker compose up -d --build              # 포트 8001
python3 -m venv .venv && .venv/bin/pip install -e ".[dev]" && .venv/bin/pytest   # 테스트
```

## 계약

관리 API는 [contracts/c4-admin-api.json](../contracts/c4-admin-api.json), 에이전트 연결(MCP 도구)은
C1 계약. 계약이 바뀌면 `contracts/`의 스키마·픽스처를 먼저 갱신한다.
