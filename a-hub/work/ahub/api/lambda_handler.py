"""AWS Lambda 진입점 — Mangum으로 FastAPI(ASGI)를 API Gateway에 연결.

SAM이 `SPACE_A_TABLE`을 주면 create_app → make_store가 DynamoDB를 쓴다.
Lambda는 REST만 노출한다(mount_mcp=False). MCP(/mcp)는 별도 상시 컨테이너에서 띄운다.
"""

from mangum import Mangum

from .rest_server import create_app

handler = Mangum(create_app(mount_mcp=False))
