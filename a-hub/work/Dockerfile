# syntax=docker/dockerfile:1
FROM python:3.12-slim

WORKDIR /app

# 사내망(인증서 가로채는 프록시)에서 빌드할 때만 신뢰 호스트를 넘긴다. 기본값은 비어 있음(보안 기본).
#   docker build --build-arg PIP_TRUSTED="--trusted-host pypi.org --trusted-host files.pythonhosted.org" -t space-a-hub .
ARG PIP_TRUSTED=""

# 서버(프로덕션) 런타임 의존성만. pyproject의 [project].dependencies와 일치시킨다.
# 서버리스 전용(mangum/boto3, [serverless] extra)은 여기 들어오지 않는다 — 서버/서버리스 분리.
# mcp는 필수다: CMD가 create_app을 기본값(mount_mcp=True)으로 띄우므로 없으면 부팅 자체가 실패한다.
# 상한 <2: mcp 2.0에서 FastMCP가 MCPServer로 개명되며 mcp.server.fastmcp 모듈이 사라졌다.
RUN pip install --no-cache-dir ${PIP_TRUSTED} "fastapi>=0.115" "uvicorn>=0.30" "mcp>=1.27,<2"

# 애플리케이션 코드 (ports & adapters 전부)
COPY ahub ./ahub

EXPOSE 8000

# create_app 은 FastAPI 인스턴스를 반환하는 팩토리
CMD ["uvicorn", "ahub.api.rest_server:create_app", "--factory", "--host", "0.0.0.0", "--port", "8000"]
