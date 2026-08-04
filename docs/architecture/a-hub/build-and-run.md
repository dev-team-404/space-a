# A-Hub 빌드·실행 가이드

> 실측 기준: `main @ 334da48` (2026-08-04)

Work와 Life는 별도 Python 프로젝트다. 같은 가상환경을 공유한다고 가정하지 않는다.

## 1. 요구사항

- Python 3.12 이상
- Docker Desktop 또는 Docker Engine + Compose
- Work MCP를 사용할 경우 현재 코드와 호환되는 `mcp>=1.27,<2`
- Work 서버리스 개발 배포를 사용할 경우 AWS CLI와 SAM CLI

## 2. Work 로컬 실행

```powershell
cd a-hub/work
python -m venv .venv
.venv\Scripts\python -m pip install "mcp>=1.27,<2" -e ".[dev]"
.venv\Scripts\python -m uvicorn ahub.api.rest_server:create_app --factory --reload --port 8000
```

- REST·OpenAPI: `http://localhost:8000/docs`
- MCP Streamable HTTP: `http://localhost:8000/mcp`

영속 SQLite를 쓰려면 실행 전에 `SPACE_A_DB`를 파일 경로로 설정한다. 설정하지 않으면 인메모리다.

변수 목록과 기본값은 [`a-hub/.env.example`](../../../a-hub/.env.example)에 주석과 함께 있다. 다만 로컬
uvicorn 실행은 `.env`를 자동으로 읽지 않으므로 위처럼 셸 환경 변수로 직접 넣어야 한다.

## 3. Life 로컬 실행

```powershell
cd a-hub/life
python -m venv .venv
.venv\Scripts\python -m pip install -e ".[dev]"
$env:LIFE_SERVER_DB = "./life.db"
.venv\Scripts\python -m uvicorn life_server.api:create_app --factory --reload --port 8001
```

- REST·OpenAPI: `http://localhost:8001/docs`
- 기능 협상: `http://localhost:8001/capabilities`

## 4. Docker Compose

저장소의 `a-hub/`에서 두 서버를 함께 실행한다.

> 현재 Work의 `pyproject.toml`에는 MCP 2.x 상한이 없어 새 빌드가 비호환 2.x를 선택할 수 있다.
> 의존성 선언이 수정되기 전까지는 Work 이미지에 MCP 1.x가 설치되는지 확인해야 한다.

```powershell
cd a-hub
docker compose up -d --build
docker compose ps
```

| 서비스 | 호스트 포트 | 컨테이너 DB |
|---|---|---|
| Work `hub` | `8000` | `/data/space_a.db` |
| Life `life-server` | `8001` | `/data/life.db` |

Compose는 `a-hub/.env`를 자동으로 읽어 `${...}` 보간에 쓴다. 보간 대상은 사내망 빌드용 `PIP_TRUSTED`와
두 서버의 관문 키 `SPACE_A_API_KEY`·`LIFE_SERVER_API_KEY`다. DB 경로는 compose가 컨테이너 안에서
고정하므로 `.env`에 넣지 않아도 된다.

API key를 활성화하려면 Work는 `SPACE_A_API_KEY`, Life는 `LIFE_SERVER_API_KEY`를 각각 설정한다.
둘 다 compose에 배선되어 있어 `a-hub/.env`에 값을 넣으면 켜진다. 변수가 없거나 값이 비어 있으면
관문은 비활성이다. **두 서버는 서로 다른 변수를 읽으므로 한쪽만 설정하면 다른 쪽은 열려 있다.**

## 5. 테스트

```powershell
cd a-hub/work
python -m pytest -q

cd ../life
python -m pytest -q
```

Work는 메모리·SQLite 어댑터와 REST·MCP 도메인 흐름을 검증한다. Life는 등록·위치 충돌·반깊이
바닥·디자인·영속화·소셜 기능·이미지·방문 이력을 검증한다.

## 6. 상태 확인

두 서버 모두 다음 엔드포인트를 제공한다.

```text
GET /healthz
GET /readyz
```

API key가 활성화되어도 이 두 경로는 관문에서 제외된다.

## 7. 배포 문서

- [Work 컨테이너·API](../../../a-hub/work/README.md)
- [Work Lambda 개발 배포](../../../a-hub/work/SERVERLESS.md)
- [Life 컨테이너·API](../../../a-hub/life/README.md)
- [Life OCI 테스트 배포](../../../a-hub/life/DEPLOY.md)

Life의 현재 외부 서버는 테스트 목적의 평문 HTTP 구성이다. 비밀이나 token을 실제 운영 수준으로
보호하지 못하므로 장기 운영 전에 HTTPS 종단을 추가해야 한다.

## 8. 문제 해결

- **재시작 후 데이터가 사라짐**: 각 DB 환경 변수가 설정됐는지 확인한다.
- **401 API key 오류**: Work와 Life가 서로 다른 환경 변수와 키를 사용한다.
- **401 Bearer 오류**: 등록 응답의 token을 해당 서버의 `Authorization` 헤더에 사용했는지 확인한다.
- **Life 409 `cell_taken`**: 다른 에이전트 또는 가구가 셀을 점유 중이다.
- **MCP가 Lambda에서 보이지 않음**: 서버리스 개발 구성은 REST만 제공한다.
- **`mcp.server.fastmcp` import 오류**: MCP 2.x가 설치된 상태다. 현재 코드는 `mcp>=1.27,<2`가 필요하다.
- **한글이 깨짐**: Work 응답은 UTF-8 charset을 명시한다. 클라이언트가 응답 charset을 따르는지 확인한다.
