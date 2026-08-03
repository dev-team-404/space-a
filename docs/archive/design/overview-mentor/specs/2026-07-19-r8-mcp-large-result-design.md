---
status: done
archived: 2026-08-03
---

# R8 MCP 대형 결과 — 유예 승격 (검토 + 설계)

> **목표**: "이 MCP 서버는 매 호출마다 큰 결과를 가져와 컨텍스트를 크게 소모한다"는 실제 토큰 낭비를 코칭.
> 관련: [기능 표 R8](../02-features.md) · [로드맵 §5](../04-history-and-roadmap.md)

## 0. 30초 요약

- **결정**: `tool_result`에 상태(Ok/Denied/Error)에 이어 **결과 크기 `result_len`(문자 수)** 를 수집.
  서버별로 큰(≥8000자) 결과가 반복(≥3회, 14일)되면 R8 발화.
- **정밀도의 선**: 판별·수치는 전부 결정론(측정된 평균/누적 문자 → 근사 토큰). `est_tokens_saved=0`(가치제안형) —
  실제 절약은 사용자가 얼마나 좁히냐에 달려 강변하지 않는다. evidence에 **측정값**만 담아 서사가 인용.

## 1. 왜 이제 가능한가

기존 수집은 `result_status`(enum)만 있어 "폐기(실패)" 여부는 알아도 "크기"는 몰랐다("승격의 절반은 준비").
어댑터가 tool_result content를 상태 분류용으로 이미 평탄화(`tool_result_content_string`)하므로,
그 문자열 길이를 함께 저장하면 크기 신호가 완성된다 — 본문은 **저장하지 않는다**(파생 신호만).

## 2. 수집 변경

| 계층 | 변경 |
|---|---|
| `model.rs` | `EventKind::ToolResult { tool_use_id, status, result_len }` — 필드 추가 |
| `adapter.rs` | `result_len = cstr.chars().count()` (평탄화된 결과 문자 수) |
| `store.rs` | `events.result_len INTEGER` 컬럼 + 마이그레이션(부재 시 추가·재수집 백필) |

## 3. 판별 (결정론)

`tool_result(r).result_len` ↔ 같은 세션 `tool_call(c).tool_server` 조인 → 서버별 집계:

```
SELECT c.tool_server, COUNT(*), SUM(r.result_len), MAX(r.result_len)
FROM events r JOIN events c ON c.tool_use_id=r.tool_use_id AND c.kind='tool_call' AND c.session_id=r.session_id
WHERE r.kind='tool_result' AND c.tool_server IS NOT NULL AND r.result_len >= 8000 AND r.ts >= :since
GROUP BY c.tool_server HAVING COUNT(*) >= 3
```

- **scope**: `mcp:<server>`, severity `Suggest`, `est_tokens_saved=0`.
- **evidence**: server, large_result_count, avg_chars, max_chars, approx_tokens_avg/total, threshold.
- **처방(kind=`narrow_mcp_result`)**: "필요한 필드만 요청/결과 좁히기(페이지네이션·요약·필터)".

## 4. 왜 "폐기"가 아니라 "대형"으로 v1을 잡나

정확한 "폐기"는 결과 직후 compaction 경계와의 상관이 필요한데(경계는 `Compaction` 이벤트로 수집됨),
상관 규칙은 취약하다. v1은 **반복되는 대형 결과**라는 견고하고 방어 가능한 신호로 시작한다 —
큰 MCP 결과는 압축 여부와 무관하게 매 호출 컨텍스트를 실제로 소모하므로 그 자체로 코칭 가치가 있다.
compaction 상관("가져오자마자 버려짐")은 후속.

## 5. 검증 (2026-07-19, 실경로 E2E)

- 합성 세션: `mcp__context7__get-library-docs` 호출 3회 + 각 12000자 tool_result → ingest.
- `rules`: R8 발화 — `server:context7, large_result_count:3, avg_chars:12000, approx_tokens_avg:3000, approx_tokens_total:9000`.
  (JSONL 결과 문자 → adapter `result_len` → store → 조인까지 전 구간 관통 확인.)
- 단위 테스트: 발화/문턱 미달 침묵/비-MCP 대형결과 무시(3), `finding_advice` 서버·측정토큰 인용(1).
  core 313 + app 25 + vitest 89 그린.
