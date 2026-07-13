#!/usr/bin/env bash
# 이 레포의 실제 PR 활동을 live-data.js 스냅숏으로 갱신한다.
# 필요: gh CLI (+ 서사 번역용 claude CLI — 없으면 규칙 기반 폴백)
set -euo pipefail
cd "$(dirname "$0")"
exec node update-live.mjs
