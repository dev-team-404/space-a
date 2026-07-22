# a-lens 단일 컨테이너 — 프론트를 빌드해 백엔드(uvicorn)가 /에서 정적 서빙한다.
# 빌드 컨텍스트는 a-lens/ (backend·frontend·assets 포함). 사내 proxy CA는 proxy-certs 컨텍스트로 주입.
# syntax=docker/dockerfile:1

# ── 1) 프론트 빌드 (Vite) ──
FROM node:20-alpine AS frontend
WORKDIR /fe
# 사내 proxy CA 인증서(proxy-certs 컨텍스트의 *.crt)가 있으면 CA 번들에 append (없으면 no-op).
# node:alpine엔 update-ca-certificates가 없어 번들에 직접 붙인다 → npm은 NODE_EXTRA_CA_CERTS를 따름.
RUN --mount=type=bind,from=proxy-certs,target=/tmp/proxy-certs \
    if ls /tmp/proxy-certs/*.crt 1>/dev/null 2>&1; then \
      cat /tmp/proxy-certs/*.crt >> /etc/ssl/certs/ca-certificates.crt; \
    fi
ENV NODE_EXTRA_CA_CERTS=/etc/ssl/certs/ca-certificates.crt
COPY frontend/package.json frontend/package-lock.json ./
RUN npm install
COPY frontend/ ./
RUN npm run build          # → /fe/dist

# ── 2) 백엔드 런타임 (+ 빌드된 프론트) ──
# main.py가 parents[2](= /app/a-lens) 기준으로 frontend/dist·assets를 찾으므로 레포 레이아웃을 재현한다.
FROM python:3.12-slim AS runtime
WORKDIR /app/a-lens
# 사내 proxy CA 인증서를 신뢰 저장소에 설치 (pip는 REQUESTS_CA_BUNDLE을 따름).
RUN --mount=type=bind,from=proxy-certs,target=/tmp/proxy-certs \
    if ls /tmp/proxy-certs/*.crt 1>/dev/null 2>&1; then \
      cp /tmp/proxy-certs/*.crt /usr/local/share/ca-certificates/ && update-ca-certificates; \
    fi
ENV PYTHONUNBUFFERED=1 PIP_NO_CACHE_DIR=1 \
    SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt \
    REQUESTS_CA_BUNDLE=/etc/ssl/certs/ca-certificates.crt \
    CURL_CA_BUNDLE=/etc/ssl/certs/ca-certificates.crt
COPY backend/ ./backend/
RUN pip install -e ./backend          # editable — main.py __file__(/app/a-lens/backend/alens) 경로 유지
COPY assets/ ./assets/
COPY --from=frontend /fe/dist ./frontend/dist
WORKDIR /app/a-lens/backend
EXPOSE 8600
CMD ["uvicorn", "alens.main:create_app", "--factory", "--host", "0.0.0.0", "--port", "8600"]
