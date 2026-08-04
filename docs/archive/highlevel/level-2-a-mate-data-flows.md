# A-Mate ↔ A-Hub 데이터 흐름 (Level 2)

> a-mate가 허브에 **언제, 무엇을, 어떻게** 보내고 받는지, 그리고 **잘 나가고 있는지 확인하는 법**.
> 코드 실측 기준: `main` @ `3a6981a` (2026-08-03).
> 상세 설계: [지식 공유](../design/a-mate/specs/2026-07-18-hub-knowledge-sharing-design.md) ·
> [텔레메트리](../design/a-mate/specs/2026-07-18-hub-telemetry-design.md) ·
> [세션 회고](../design/a-mate/specs/2026-07-19-session-retro-knowledge-design.md) ·
> [지식 재사용 루프](../design/common/specs/2026-07-25-close-knowledge-reuse-loop-design.md)

## 한눈에 — 3개 송신 + 2개 수신

전부 **스캔 파이프라인이 끝난 뒤 편승**하며, 실패해도 앱은 무해(다음 스캔 재시도).

| 채널 | 언제 | 무엇을 | 어디로 |
|---|---|---|---|
| **① 코칭 지식** | 스캔 후, 유의미한 발견이 새로 생겼을 때. 건당 평생 1회, 스캔당 ≤3건 | **R8만** — 서버명·건수·문자수뿐이라 개인 텍스트가 없고, 같은 MCP를 쓰는 팀에 그대로 통한다 | `knowledge_hub_space_id` 공간 |
| **② 텔레메트리** | 하루 1회, 자정 지나 첫 스캔(어제치). 활동 0인 날은 침묵 | 세션 수·토큰 합계·모델 비율·코칭 상태·MCP 호출 수 — **숫자만** | `a-mate-telemetry` 공간 (a-lens 원료) |
| **③ 세션 회고** | 세션 종료 후, 오류를 여러 번 겪고 회복한 세션만. 하루 상한 있음, 세션당 평생 1회 | Engine이 쓴 2~3문장 **로컬 생성 요약** (엔진 없으면 보류) | `knowledge_hub_space_id` 공간 |
| **④ 팀 지식** (수신) | 6시간마다 | 팀원이 발행한 최신 페이지(내 것 제외) → "오늘의 배움" 카드 | — |
| **⑤ 인정** (수신) | 매 스캔 | 내가 발행한 페이지를 **남이 인용한** 기록 → 마스코트 축하 말풍선 | — |

### ① 선별 기준

`화이트리스트 ∩ status=new ∩ 문턱 통과 ∩ 미공유`, 상한 `MAX_PER_SCAN`. (`hub.rs` 상수)

| 상수 | 값 |
|---|---|
| `SHARE_RULES` | `["R8"]` |
| `DEFAULT_MIN_TOKENS` | 1000 (`est_tokens_saved` 하한) |
| `MIN_OCCURRENCES_WHEN_NO_EST` | 3 (추정치 없는 룰의 대체 문턱) |
| `MAX_PER_SCAN` | 3 |

**왜 R8만인가**

- **R8** — 증거가 서버명·건수·문자수뿐이라 개인 텍스트가 없다. 같은 MCP를 쓰는 팀에 그대로 적용된다 ✅
- R6 — `repeated_prompt`가 프롬프트 원문이라 스크럽하면 의미가 사라진다 ❌
- R7 — 개인의 세션 단위 모델 선택이라 팀 지식이 아니다 ❌

### ① 발행 경로 — 무조건 올리지 않는다

```
발견 1건
  └─ POST /pages/search   (마커로 팀에 같은 지식이 이미 있는지)
       ├─ 있음 → POST /issues → POST /issues/{id}/cite    ← 중복 발행 대신 인용
       └─ 없음 → POST /issues → POST /issues/{id}/resolve ← 새 지식 발행
```

**마커는 이슈 제목이 아니라 `summary`에 심는다.** 허브의 `resolve_issue`가 발행 페이지 제목을
`title=summary`로 만들기 때문에, 이슈 제목에 넣으면 나중에 되찾을 수 없다.

### ⑤ 인정 루프

```
GET /reuse-events?limit=200
  → page_id가 내 발행분인 행만 · cited_by ≠ 나 · created_at > cursor
  → reuse:celebrated
```

커서 규약이 dedup을 대신한다. 첫 실행에는 emit 없이 커서만 초기화(설치 직후 도배 방지),
커서는 emit 성공 시에만 전진 → **한 이벤트는 평생 1회만** 축하된다.

> ⚠ **운영 허브가 아직 `/reuse-events`를 404로 응답한다.** 그동안 이 채널은 조용히 no-op이고,
> 404를 만나면 프로세스 수명 동안 스킵하므로 **허브 배포 후 앱 재시작이 필요하다.**

### 개인정보 경계

대화 원문·파일 경로·세션 ID·cwd는 **어떤 채널로도 나가지 않는다.**
회고의 "작업 한 줄"은 사용자가 고른 Engine까지만 가고, 허브엔 요약 텍스트만 도착한다.

## 켜고 끄기

설정 해석 우선순위는 **설정(store) → 환경변수 → 팀 기본값**이다.

```
knowledge_hub_share = off   → 유일한 opt-out. 설정·env·기본값 어떤 경로로도 되살아나지 않는다
SPACE_A_SHARE=off           → 환경변수 경로만 off (설정·기본값은 그대로 산다)
SPACE_A_RETRO=off           → 회고만 off
```

> ⚠ **`SPACE_A_HUB_URL`을 안 지정해도 붙는다.** 2026-07-28부터 팀 기본값이 들어갔다 —
> `https://spacea.msalt.net` / 공간 `sw-innov`. 설치 직후 바로 팀 지식을 주고받게 하려는 의도다.
> **API 키만 기본값이 없다**(공유 비밀을 소스에 넣지 않기 위해) — 설정 탭에서 1회 입력한다.
> 키가 없으면 허브 호출이 401로 실패하지만 "실패 무해" 규율이라 앱 동작에는 지장이 없다.

`.env`는 `#[cfg(debug_assertions)]`에서만 로드되므로 **배포 빌드는 환경변수만으로 연결되지 않는다.**
배포본에서는 설정 창 입력이 유일한 경로다.

토큰은 최초 필요 시 자동 register 후 로컬 settings(`knowledge_hub_token`)에만 저장된다.

## 잘 나가고 있는지 확인하는 법

**1) 앱 로컬 장부 (가장 정확)** — 무엇이 나갔는지의 단일 원천:

```sql
-- %APPDATA%\dev.agentmentor.app\agent-mentor.db
SELECT * FROM hub_share_state ORDER BY shared_at;
-- dedup_key 형식: {finding dedup} | telemetry|YYYY-MM-DD | retro|{session_id}
-- page_id가 채워져 있으면 발행 완료, issue_id만 있으면 다음 스캔에 resolve 재시도
```

**2) 발행 전 단계 진단** — "왜 안 나갔지?"는 대부분 여기서 답이 나온다:

```sql
SELECT rule_id, status, est_tokens_saved, occurrences FROM findings WHERE rule_id='R8';
```

R8 발견이 0이면 ①은 침묵이 **정상**이다(보낼 가치가 없음).

**3) 허브 쪽에서 확인**:

```sh
curl -H "x-api-key: $KEY" -H "Authorization: Bearer $TOKEN" \
  "$HUB/issues?space_id=$SPACE&mine=true"
curl -H "x-api-key: $KEY" -H "Authorization: Bearer $TOKEN" \
  "$HUB/spaces/a-mate-telemetry/tree"
curl -H "x-api-key: $KEY" -H "Authorization: Bearer $TOKEN" \
  "$HUB/reuse-events?limit=50"          # 인정 루프 — 현재 404면 미배포
```

**4) CLI로 수동 트리거** (앱 없이 즉시 확인):

```sh
agent-mentor hub-share   # ① 공유 대상 선별→발행 (0건이면 사유 출력)
agent-mentor telemetry   # ② 어제치 발행
agent-mentor retro       # ③ 회고 후보 선별→발행
agent-mentor reuse       # ⑤ 내 지식 재사용 조회
```

**5) 앱 로그**: `%LOCALAPPDATA%\dev.agentmentor.app\logs\agent-mentor.log`

## 별개 계열 — 미니홈피(life)

방·방문·방명록은 **다른 서버·다른 설정 키**(`hub_*`)로 돌아간다. 지식 허브(`knowledge_hub_*`)와
혼동하지 않는다. 상세는 [level-2-life-visit.md](./level-2-life-visit.md).
