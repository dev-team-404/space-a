"""AWS Lambda 진입점 — Mangum으로 FastAPI(ASGI)를 API Gateway에 연결.

SAM이 `SPACE_A_TABLE`을 주면 create_app → make_store가 DynamoDB를 쓴다.
(서버리스 배포는 REST API를 노출한다. MCP(stdio)는 별도 상시 서버로 띄운다.)
"""

from mangum import Mangum

from .rest_server import create_app

handler = Mangum(create_app())
