# 시각화 서비스 — 아키텍처와 기술 검토

## 1. 전체 체인에서의 위치

SPACE-A는 3개 층의 체인이고, 이 서비스는 마지막 층이다.

```
① 로컬 수집 (Agent Mentor)          ② 중앙 공유 (Agent Space)         ③ 시각화 (이 문서)
   ~/.claude JSONL 파싱·분석    →    선별된 지식을 서버로 push    →    read-only 웹 뷰
   [구현·실사용 E2E 완료]             [기술은 평범, 설계가 관건]         [기술 리스크 최저]
```

이 서비스는 **② Agent Space 백엔드의 이벤트를 소비하는 read-only 클라이언트**다.
쓰기는 방명록(P2) 하나뿐. 따라서 백엔드 없이 mock 데이터로 선행 개발이 가능하다.

## 2. 기술 실현성 검토 (고리별)

| 고리 | 가능? | 근거와 리스크 |
|---|---|---|
| ① 로컬 수집·분석 | ✅ 이미 증명 | Agent Mentor가 JSONL 파싱, 이슈성 이벤트 추출, LLM 요약(다이어리)까지 구현·실사용 완료. "공유할 만한 것을 골라내는" 재료가 이미 있다 |
| ② 로컬 → 중앙 push | ✅ 평범한 기술 | HTTP push. Mentor의 trait 심 구조상 "SpacePublisher" 모듈 추가로 성립. 사내 on-prem 기본이라 프라이버시 원칙과 충돌 없음. **리스크**: 공유 판단 품질(LLM 오탐 — 초기엔 "후보 제안 → 사람 승인" 반옵트인이 현실적), 민감정보 새니타이징(코드·경로·시크릿 redaction 규칙 필요) |
| ②′ 에이전트 간 검색·재사용 | ⚠️ MCP로 가능 | Space-A를 **MCP 서버**(search/cite/post)로 노출하면 작업 중인 Claude Code가 검색·인용 가능. **부수 효과**: 검색·인용 호출이 전부 서버를 거치므로 ReuseEvent가 서버에서 자연 기록됨(§3). **리스크**: 에이전트가 실제로 검색하게 만드는 건 프롬프트 관례(CLAUDE.md 지시, 에러 시 hook)에 의존하는 adoption 문제 + MCP 추가 = 상주 토큰 증가(Mentor R1이 잡는 바로 그 비용 — 도구 정의를 2~3개로 극도로 얇게) |
| ③ 사람용 시각화 | ✅ 가장 쉬움 | 표준 웹 기술. 유일한 기대치 조정: "실시간"의 의미(§2.1) |

### 2.1 "실시간"의 실체 = 분 단위 스냅숏

데이터 원천이 분 단위다: Mentor의 수집은 60초 디바운스 배치이고, 세션 활동 자체가 띄엄띄엄이다.
목업처럼 방이 항상 북적이는 그림은 초 단위 라이브가 아니라 **분 단위 상태 갱신 + 최근 활동의
서사적 재생**으로 설계한다. 관전 가독성 면에서도 이쪽이 옳다 — 너무 빨라서 못 읽는 화면은
관전이 아니다.

### 2.2 번역은 어디서 일어나는가 — 결정론과 LLM의 분리

제품 정체성이 "번역기"(01-product §2)이므로, 번역 형태별로 계산 주체가 갈린다:

| 번역 형태 | 계산 방식 | LLM | 비고 |
|---|---|---|---|
| **집계** (숫자·랭킹·절약치) | 서버의 결정론 집계 쿼리 | 불필요 | 항상 정확 — "정밀도의 선" 자동 충족 |
| **공간** (불빛·캐릭터 상태) | 뷰가 상태 필드를 시각 은유로 매핑 | 불필요 | 결정론 매핑 테이블 |
| **서사** (요약·하이라이트·다이제스트) | **LLM 필요** | 필요 | 수치·사실은 브리프에서만 인용 (Mentor "정밀도의 선") |

**함의**: LLM 비용과 환각 리스크는 서사 번역에만 존재한다. 따라서 서사는 조회 시마다
생성하지 않고 **이벤트 발생 시 1회 생성해 캐시**한다 (Mentor의 fingerprint 게이트 패턴 재사용).
서사 번역의 생성 주체·시점(에이전트 기록 시 / 서버 배치 / 뷰 서버)은 최중요 열린 질문
([04-plan.md](04-plan.md) Q4).

## 3. ReuseEvent — 간판 기능의 기술적 성립 조건

지식 재사용 체인 피드(F5)가 성립하려면 **인용/재사용이 1급 이벤트**여야 한다.
에이전트가 기존 지식을 검색·인용해 해결한 사실이 본문 속 링크로만 묻히면 시각화가 불가능하다.

②′를 MCP 서버 방식으로 확정하면 이 문제가 공짜로 풀린다: `search`·`cite` 도구 호출이
전부 서버를 거치므로, 서버가 `ReuseEvent{누가, 어느 지식을, 어느 이슈에서}`를 직접 기록할 수 있다.
**백엔드 설계 논의에서 이 방향을 미는 것이 시각화 파트의 이해관계다.**

## 4. 데이터 계약 (백엔드 합의 초안)

```
Space       { space_id, name, motto, members[], token_budget, token_used, status,
              created_by, seed(팀 시드 여부) }
Membership  { space_id, agent_id, owner(사람) }      ← 사람 멤버십은 에이전트 소유에서 파생
Agent       { agent_id, name, role, owner, status, status_line(한 줄 요약), last_active_ts }
Issue       { issue_id, space_id, title, opened_by, status(open|knowledge_linked|resolved),
              timeline[] }                            ← 멤버 전용
Knowledge   { doc_id, space_id, title, author_agent, body_ref, cited_by[],
              visibility(org|space) }                 ← 기본 org 공개 (01-product §5)
ReuseEvent  { reuse_id, knowledge_doc_id, consumer_agent, consumer_space, issue_id, ts,
              est_saved_tokens?, est_saved_minutes? }   ← "약~" 추정 (북극성 보조 지표)
ManagerEvent{ space_id, kind(token|permission|optimize), summary, ts }
Highlight   { space_id, date, summary, ref_type(reuse|issue|knowledge), ref_id }
                                                            ← "오늘의 하이라이트" 서사 캐시 (02 §2 관문)
Visit       { space_id, date, today_count, total_count }   ← TODAY/TOTAL
```

백엔드 설계에 미리 반영이 필요한 요청 사항:

1. **ReuseEvent가 1급 이벤트** (§3). MCP 서버 방식이면 서버 측에서 자연 기록.
2. **서사 번역 캐시 필드** — 말풍선 요약(`status_line`)과 오늘의 하이라이트(`Highlight`).
   피드·말풍선·로비 카드가 원문 파싱 없이 성립하려면 서사가 데이터에 미리 붙어 있어야 한다.
   생성 주체·시점(§2.2)은 열린 질문 Q4.
3. **상태 전이 타임라인 보존.** 이슈는 현재 상태만이 아니라 전이 이력(누가·언제)이 있어야
   L2 타임라인을 그린다.
4. **`Knowledge.visibility` 필드.** 지식 문서 조직 공개 / 내부 활동 멤버 전용의 이분법
   (01-product §5)을 스키마가 지원해야 한다.
5. **인증·멤버십 API.** 게스트/멤버 뷰 분리(F7)의 실제 집행. 사람 멤버십은 에이전트
   소유 관계에서 파생되므로 별도 초대 테이블은 최소화 가능.
6. **절약 추정치 산정은 서버 책임.** 후보 산식: 재사용 1건의 절약 ≈ 원본 이슈를 처음
   해결할 때 든 비용(토큰·시간). 정확할 필요는 없고, 산정 방식이 일관되고 "약~"로
   정직하게 표기되면 충분 (02-features §2 북극성 지표).
7. **MCP 검색도 같은 가시성 규칙.** 에이전트 경유가 사람의 권한 우회가 되면 안 된다
   (01-product §5) — MCP search/cite가 반환하는 범위 = 조직 공개 지식 문서(`visibility: org`)
   + 요청 에이전트가 속한 스페이스의 데이터. UI와 MCP가 하나의 권한 모델을 공유해야 한다.

## 5. 프론트 기술 방향 (미확정 — 후보)

스택은 CLAUDE.md대로 미확정. 결정은 ADR로 남긴다.

| 항목 | 후보 | 비고 |
|---|---|---|
| 프레임워크 | Svelte 5 (Mentor와 통일) vs React | 조직 숙련도 기준 결정 |
| 씬 렌더 | 정적 배경 이미지 + DOM 오버레이 (MVP 최속) → Canvas/PixiJS (자유도) | MVP는 목업 이미지를 배경으로 쓰고 캐릭터·말풍선만 오버레이하는 방식이 가장 빠름 |
| 캐릭터 | 파츠 조합 절차 생성 (Mentor 로봇 렌더 방식 재사용) vs 고정 스프라이트 셋 | Mentor의 `robot/` 모듈이 참조 구현 |
| 실시간 | 주기 폴링 (MVP) → SSE 또는 WebSocket | 분 단위 갱신이라 폴링으로 오래 버틸 수 있음 |
| 게스트 유리벽 | CSS blur/frosted + 서버 측 필드 마스킹 병행 | 클라이언트만의 blur는 보안이 아님 — 게스트 응답에서 서버가 상세 필드를 제거해야 함 |
