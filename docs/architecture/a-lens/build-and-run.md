# A-Lens — 빌드와 실행

> 실측 기준: `main @ fda2467` (2026-08-04)

## 1. 요구 사항

- Python 3.11 이상
- Node.js 20 계열과 npm
- 선택: Docker Compose
- 실데이터 사용 시 A-Hub Work 주소·Bearer token·선택적 API key
- 선택: A-Hub Life 주소·token·API key
- 선택: OpenAI 호환 LLM 서버

## 2. 개발 실행

백엔드:

```powershell
cd a-lens\backend
python -m venv .venv
.\.venv\Scripts\python -m pip install -e ".[dev]"
.\.venv\Scripts\python -m uvicorn alens.main:create_app --factory --port 8600 --reload
```

프론트는 별도 터미널에서 실행한다.

```powershell
cd a-lens\frontend
npm install
npm run dev
```

- 프론트: `http://localhost:5279`
- 백엔드: `http://localhost:8600`
- 상태 확인: `http://localhost:8600/api/health`

Vite는 `/api`, `/assets`를 기본 8600 포트로 프록시한다. 다른 백엔드 포트를 쓰면 프론트 실행 전에
`A_LENS_API_PORT`를 지정한다.

## 3. 설정

`backend/.env.example`을 참고해 환경 변수를 셸에 넣거나, 앱 홈의 설정 창에서 저장한다. `.env` 파일은
Python 코드가 자동으로 읽지 않는다. 저장 설정은 기본적으로 `backend/.a-lens/settings.json`에 기록되며 환경 변수보다 우선한다.

주요 설정:

| 변수 | 의미 | 기본 |
|---|---|---|
| `A_LENS_SOURCE` | `auto`, `hub`, `dummy`, `fixtures` | `auto` |
| `A_LENS_WORK_URL` | A-Hub Work 주소 | `https://spacea.msalt.net` |
| `A_LENS_WORK_TOKEN` | Work Bearer token | 없음 |
| `A_LENS_WORK_API_KEY` | Work `x-api-key` | 없음 |
| `A_LENS_LIFE_URL` | A-Hub Life 주소. 비우면 연동 off | 없음 |
| `A_LENS_LIFE_TOKEN` | Life Bearer token | 없음 |
| `A_LENS_LIFE_API_KEY` | Life `x-api-key` | 없음 |
| `A_LENS_LIFE_ALIAS` | `닉네임=work_id|work_id` 별칭 목록 | 없음 |
| `A_LENS_PRESENCE_WINDOW` | Work 최근 활동을 online으로 볼 초 | `3600` |
| `A_LENS_CACHE_TTL` | Work 스냅숏 갱신 초 | `30` |
| `A_LENS_LLM_URL` | OpenAI 호환 base URL. 비우면 번역 off | 로컬 LM Studio 주소 |
| `A_LENS_LLM_MODEL` | 모델명 | 저장소 기본 모델명 |
| `A_LENS_LLM_KEY` | LLM Bearer key | 없음 |
| `A_LENS_SUMMARY_STYLE` | `brief`, `normal`, `detailed` | `brief` |
| `A_LENS_DB` | 번역 캐시 SQLite 경로 | `.a-lens/translation.db` |

설정 API는 비밀값 원문을 돌려주지 않고 `*_set`만 반환한다. 다만 `settings.json` 자체는 평문 파일이므로
커밋하거나 공유하지 않는다.

## 4. 정적 빌드

```powershell
cd a-lens\frontend
npm run build
```

TypeScript 검사를 통과한 뒤 `frontend/dist`를 만든다. 이 디렉터리가 있으면 FastAPI가 `/`에서 정적 프론트를 서빙한다.

## 5. Docker

`a-lens/backend/.env`에 필요한 연결값을 준비한 뒤 WSL 또는 Docker가 동작하는 셸에서 실행한다.
Compose가 인증서용 추가 빌드 컨텍스트를 항상 참조하므로 `.proxy-certs/` 디렉터리는 비어 있어도 존재해야 한다.
저장소에는 이를 위한 `.gitkeep`이 포함되어 있다.

```sh
cd a-lens
docker compose up -d --build
```

한 컨테이너가 `0.0.0.0:8600`에서 API와 프론트를 함께 제공한다. `alens-data` 볼륨은
`/app/a-lens/backend/.a-lens`를 보존한다. 사내 프록시 CA가 필요하면 `a-lens/.proxy-certs/`에 `.crt` 인증서를 둔다.

## 6. 검증

백엔드 테스트:

```powershell
cd a-lens\backend
.\.venv\Scripts\python -m pytest
```

프론트 타입 검사와 빌드:

```powershell
cd a-lens\frontend
npm run build
```

런타임 확인:

```powershell
Invoke-RestMethod http://localhost:8600/api/health
Invoke-RestMethod http://localhost:8600/api/lobby
```

화면에 실데이터가 없을 때는 먼저 홈의 `FAKE` 배지와 설정의 `source`를 확인한다. `auto`는 Work 실패를 더미로
강등하므로 화면이 열린다는 사실만으로 Work 연결 성공을 뜻하지 않는다. 설정 창의 연결 테스트 또는 서버 로그를 함께 본다.
