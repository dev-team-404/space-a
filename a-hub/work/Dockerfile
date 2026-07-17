# syntax=docker/dockerfile:1
FROM python:3.12-slim

WORKDIR /app

# 사내망(인증서 가로채는 프록시)에서 빌드할 때만 신뢰 호스트를 넘긴다. 기본값은 비어 있음(보안 기본).
#   docker build --build-arg PIP_TRUSTED="--trusted-host pypi.org --trusted-host files.pythonhosted.org" -t space-a-hub .
ARG PIP_TRUSTED=""

# 런타임 의존성만 설치 (dev 도구 pytest/httpx 제외). pyproject의 dependencies와 맞춘다.
RUN pip install --no-cache-dir ${PIP_TRUSTED} "fastapi>=0.115" "uvicorn>=0.30"

# 애플리케이션 코드 (ports & adapters 전부)
COPY ahub ./ahub

EXPOSE 8000

# create_app 은 FastAPI 인스턴스를 반환하는 팩토리
CMD ["uvicorn", "ahub.api.rest_server:create_app", "--factory", "--host", "0.0.0.0", "--port", "8000"]
