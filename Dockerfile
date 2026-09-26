FROM --platform=$BUILDPLATFORM node:24-bookworm-slim AS dashboard
WORKDIR /build/frontend
COPY frontend/package.json frontend/package-lock.json ./
RUN npm ci
COPY frontend/ ./
COPY lang/ /build/lang/
RUN npm run build

FROM python:3.12-slim-bookworm
COPY --from=ghcr.io/astral-sh/uv:0.12.19 /uv /usr/local/bin/uv
ARG BUILD_DATE
ARG VCS_REF
ARG VERSION
LABEL org.opencontainers.image.created="${BUILD_DATE}" \
      org.opencontainers.image.authors="rangermix, ohne-b" \
      org.opencontainers.image.source="https://github.com/ohne-b/twitch-miner" \
      org.opencontainers.image.version="${VERSION}" \
      org.opencontainers.image.revision="${VCS_REF}" \
      org.opencontainers.image.licenses="MIT" \
      org.opencontainers.image.title="Twitch Drops Miner"
ENV PYTHONUNBUFFERED=1 \
    PYTHONDONTWRITEBYTECODE=1 \
    UV_PROJECT_ENVIRONMENT=/app/env \
    VIRTUAL_ENV=/app/env \
    PATH="/app/env/bin:$PATH" \
    PORT=8080
WORKDIR /app
COPY pyproject.toml uv.lock README.md LICENSE ./
RUN uv sync --locked --no-dev --no-install-project --no-cache
COPY main.py ./
COPY src/ ./src/
COPY lang/ ./lang/
COPY icons/ ./icons/
COPY --from=dashboard /build/web/ ./web/
RUN mkdir -p /app/data /app/logs
EXPOSE 8080
HEALTHCHECK --interval=30s --timeout=3s --start-period=10s --retries=3 \
    CMD python -c "import urllib.request; urllib.request.urlopen('http://localhost:8080/healthz')" || exit 1
CMD ["python", "main.py"]
