# A-Mate → A-Hub 데이터 송신 (Level 2)

> a-mate가 허브에 **언제, 무엇을, 어떻게** 보내는지와 **잘 나가고 있는지 확인하는 법**.
> 상세 설계: [지식 공유](../design/overview-mentor/specs/2026-07-18-hub-knowledge-sharing-design.md) ·
> [텔레메트리](../design/overview-mentor/specs/2026-07-18-hub-telemetry-design.md) ·
> [세션 회고](../design/overview-mentor/specs/2026-07-19-session-retro-knowledge-design.md)

## 한눈에 — 3개 송신 채널 + 1개 수신

전부 **스캔 파이프라인이 끝난 뒤 편승**하며, 실패해도 앱은 무해(다음 스캔 재시도).

| 채널 | 언제 보내나 | 무엇을 | 어떤 형태로 | 어디로 |
|---|---|---|---|---|
| **① 코칭 지식** | 스캔 후, **유의미한 발견**이 새로 생겼을 때. 건당 평생 1회, 스캔당 ≤3건 | R1·R2·R10·R11·R12 — 팀에도 통하는 룰만 (화이트리스트) | 이슈 열고→해결로 닫으며 지식 Page 발행. 제목·처방 전부 결정론 | `SPACE_A_SPACE_ID` 공간 |
| **② 텔레메트리** | **하루 1회**, 자정 지나 첫 스캔 (어제치). 활동 0인 날은 침묵 | 세션 수·토큰 합계·모델 비율·코칭 상태·MCP 호출 수 — **숫자만** | 순수 JSON Page (`a-mate-telemetry/v0`) | `a-mate-telemetry` 공간 (a-lens 원료) |
| **③ 세션 회고** | 세션 **종료 30분 후**, "오류 ≥3회 겪고 회복한" 세션만. 하루 ≤2건, 세션당 평생 1회 | 무슨 작업에서 어떤 시행착오를 겪고 어떻게 마무리됐나 | Engine이 쓴 2~3문장 **로컬 생성 요약** (엔진 없으면 보류) | `SPACE_A_SPACE_ID` 공간 |
| (수신) 팀 지식 | 매 스캔 | 팀원이 발행한 지식 최신 5건 (내 것 제외) | "오늘의 배움" 피드 아이템 | — |

**"유의미한 발견"의 기준 (①)**: `status=new` ∧ 화이트리스트 룰 ∧ (`est_tokens_saved ≥ 1000` 또는
est=0 룰은 `occurrences ≥ 3`). R6(반복 지시)는 프롬프트 원문이 evidence에 있어 **공유 제외**.

**개인정보 경계**: 대화 원문·파일 경로·세션 ID·cwd는 어떤 채널로도 나가지 않는다.
회고의 "작업 한 줄"은 사용자가 고른 Engine까지만 가고, 허브엔 요약 텍스트만 도착한다.

## 켜고 끄기

```
SPACE_A_HUB_URL 미설정  → 송·수신 전체 off (기본값 — 프라이버시 기본 로컬)
SPACE_A_SHARE=off       → 송신만 off (a-lens 등 다른 용도는 유지)
SPACE_A_RETRO=off       → 회고만 off
```

토큰은 최초 필요 시 자동 register(`user_id` 재등록=같은 계정) 후 로컬 settings
(`knowledge_hub_token`)에만 저장된다. ⚠ 2026-07-19 이전 빌드는 "공유할 발견이 없으면
영영 register가 안 돼 ②③도 침묵"하는 부트스트랩 버그가 있었다 — 현재는 ②③이 스스로 등록한다.

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
SELECT rule_id, status, est_tokens_saved, occurrences FROM findings;   -- 발견 자체가 있나
```
발견이 0이면 ①은 침묵이 **정상**이다(보낼 가치가 없음) — 세션·인벤토리가 쌓이면 생긴다.

**3) 허브 쪽에서 확인**:

```sh
curl -H "x-api-key: $KEY" -H "Authorization: Bearer $TOKEN" \
  "$HUB/issues?space_id=$SPACE&mine=true"        # [a-mate]·[a-mate 회고] 제목들
curl -H "x-api-key: $KEY" -H "Authorization: Bearer $TOKEN" \
  "$HUB/spaces/a-mate-telemetry/tree"            # [telemetry] {user} {date} 페이지들
```

**4) CLI로 수동 트리거** (앱 없이 즉시 확인):

```sh
agent-mentor hub-share      # ① 지금 기준 공유 대상 선별→발행 (0건이면 사유 출력)
agent-mentor telemetry      # ② 어제치 발행 (이미 보냈으면 dedup 안내)
agent-mentor retro          # ③ 회고 후보 선별→발행
```

**5) 앱 로그**: `%LOCALAPPDATA%\dev.agentmentor.app\logs\agent-mentor.log` — 발행/보류/재시도가 남는다.
