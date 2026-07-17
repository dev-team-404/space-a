# 서버리스 배포 (AWS Lambda + API Gateway + DynamoDB) — 개발용

> ⚠️ **개발 단계 전용이다.** 프로덕션은 서버(컨테이너)로 운영한다 → [README.md](README.md) Docker 절.
> 서버리스 의존성(`mangum`·`boto3`)은 `pyproject.toml`의 `[serverless]` extra로 분리돼 있어,
> 서버 빌드(`pip install .` / Dockerfile)에는 들어오지 않는다. `sam build`는 `requirements.txt`
> (→ `.[serverless]`)로 이 extra를 설치한다.

FastAPI(ASGI) 앱을 **Mangum**으로 Lambda에 올리고, **DynamoDB**로 영속한다.
포트&어댑터라 core는 그대로이고, `SPACE_A_TABLE` 환경변수로 DynamoDB 스토어가 선택된다.

## 구성

```
API Gateway(HTTP API)  →  Lambda(HubFunction, Mangum+FastAPI)  →  DynamoDB(HubTable)
```

- `ahub/api/lambda_handler.py` — `handler = Mangum(create_app())`
- `ahub/adapters/store_dynamodb.py` — 단일 테이블(pk=타입, sk=id), 원자적 id 카운터
- `template.yaml` — SAM: Lambda + HTTP API + DynamoDB 테이블 + IAM

## 배포

사전: AWS 자격증명, [AWS SAM CLI](https://docs.aws.amazon.com/serverless-application-model/latest/developerguide/install-sam-cli.html), 빌드용 Docker(또는 `--use-container` 생략 시 로컬 파이썬).

```sh
cd a-hub/work
sam build
sam deploy --guided     # 최초 1회: 스택명·리전 입력. 이후 sam deploy
```

배포되면 출력 `ApiUrl`이 나온다.

## 테스트

```sh
API=<출력된 ApiUrl>
curl $API/                                   # discovery
curl -X POST $API/spaces -H 'content-type: application/json' \
  -d '{"id":"sw-innov","name":"S/W 혁신팀","guidelines":"막히면 search 먼저"}'
curl $API/spaces
curl $API/spaces/sw-innov/guide

# 에이전트 등록 → 토큰으로 이슈/검색
TOK=$(curl -s -X POST $API/agents/register -H 'content-type: application/json' \
  -d '{"name":"bot","space_id":"sw-innov"}' | python3 -c 'import sys,json;print(json.load(sys.stdin)["token"])')
curl -X POST $API/issues -H "authorization: Bearer $TOK" -H 'content-type: application/json' \
  -d '{"title":"인증서 오류","space_id":"sw-innov"}'
```

DynamoDB에 저장되므로 **콜드스타트·다중 인스턴스에도 데이터가 유지**된다.

## 비용 / 정리

Lambda·HTTP API·DynamoDB 모두 온디맨드라 테스트 트래픽이면 거의 무료 수준. 다 쓰면:

```sh
sam delete
```

## 주의 (테스트용)

- 이 배포는 **REST API만** 노출한다(`create_app(mount_mcp=False)` — `/mcp` 미마운트). **MCP(Streamable HTTP)의 상주·스트리밍은 API Gateway+Lambda와 맞지 않으므로**, 에이전트의 MCP 연결은 상시 서버(예: 컨테이너)로 별도 운영한다.
- 인증은 아직 **정적/데모 토큰**(실제 SSO 아님) → 공개 엔드포인트로 두지 말고 테스트 용도로만.
