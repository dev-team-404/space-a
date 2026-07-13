// live-data.js — update-live.sh가 생성한 실제 GitHub PR 스냅숏 + 서사 번역 캐시. 직접 수정하지 말 것.
const LIVE_PRS = [
 {
  "author": {
   "id": "MDQ6VXNlcjg3ODk5MTQz",
   "is_bot": false,
   "login": "JuyoungKimmy-Kim",
   "name": "Kimmy Kim"
  },
  "body": "## 무엇을\n\n프로토타입 로비의 **1층(빈 층)에 \"SPACE-A 개발팀\" 스페이스**를 만들고, 이 레포의 실제 PR 활동으로 채웁니다.\n\n- PR 작성자 → 에이전트 캐릭터. 열린 PR이 있으면 말풍선에 **\"공간 뷰 문서 재편 중\"** 처럼 짧은 작업명, 머지만 있으면 \"~완료\"\n- PR → 이슈 흐름 타임라인 (작업 시작 → 반영 완료)\n- 매니저 코너에 \"PR 리뷰 대기 n건\"\n- 멤버십은 설계 규칙 그대로 파생 — 내 에이전트가 이 방 멤버면 나도 멤버 뷰\n\n## 어떻게\n\n레포가 private이라 브라우저 직접 호출 대신 **스냅숏 방식**입니다:\n\n```sh\n./prototype/frontend-viz/update-live.sh   # 보기 전에 실행 — PR 스냅숏 + 서사 번역 갱신\nopen prototype/frontend-viz/index.html\n```\n\n- `update-live.sh` → `update-live.mjs`: `gh`로 PR 목록을 받고, 기계 기록(커밋 컨벤션 제목)을 `claude` CLI(haiku)로 **사람용 한국어 서사로 번역**해 `live-data.js`에 저장\n- 서사는 **PR마다 1회 생성 후 캐시** — 번호·제목이 같으면 재번역하지 않음. 설계의 \"서사는 이벤트 시 1회 생성해 캐시\" 원칙 그대로이며, 열린 질문 Q1(서사 파이프라인)의 \"생성 시점 번역\" 후보를 프로토타입으로 검증한 셈\n- `claude` CLI가 없으면 규칙 기반 폴백 (프리픽스 파싱)\n- `live-adapter.js`가 로드 시 mock DB에 병합 — 스냅숏이 없어도 프로토타입은 기존 목 데이터만으로 동작\n\n## 참고\n\n- mock-data.js 원본은 헤더 주석 외 무변경, app.js는 \"1층에 스페이스가 생기면 엘리베이터 '빈 층' 버튼 숨김\" 한 곳만 수정\n- prototype README의 설계 문서 링크가 `docs/space-view/`를 가리키는데, 이는 #5 머지 후 유효해집니다\n\n🤖 Generated with [Claude Code](https://claude.com/claude-code)",
  "createdAt": "2026-07-13T15:15:57Z",
  "headRefName": "feat/prototype-live-pr-view",
  "mergedAt": null,
  "number": 6,
  "state": "OPEN",
  "title": "feat(frontend): reflect real GitHub PR activity in prototype as live space",
  "updatedAt": "2026-07-13T15:18:55Z",
  "url": "https://github.com/dev-team-404/space-a/pull/6"
 },
 {
  "author": {
   "id": "MDQ6VXNlcjg3ODk5MTQz",
   "is_bot": false,
   "login": "JuyoungKimmy-Kim",
   "name": "Kimmy Kim"
  },
  "body": "## 무엇을\n\n\"깊이 = 레벨\" 문서 구조로의 **파일럿 이관**입니다. 제 담당인 사람 뷰(구 frontend-viz) 문서 5개(~760줄)를 `docs/space-view/` 폴더의 4개 문서(~206줄)로 압축·재편했습니다.\n\n| 파일 | 레벨 | 내용 |\n|---|---|---|\n| `README.md` | **하이레벨** | 사람 뷰 기능 전체의 개괄 — 정의, 정체성(번역기), 페르소나, 북극성 지표, 진행 상태, 열린 질문 + 하위 문서 지도 |\n| `space-model.md` | **미드레벨** | 공간 구조(사옥/층)와 권한 모델(유리벽, visibility) |\n| `features.md` | **미드레벨** | 4레이어 정보 설계, 화면 구성, 기능 우선순위, 비목표 |\n| `architecture.md` | **미드레벨** | 데이터 흐름, 번역 계산 분리, 기술 후보 |\n\n로우레벨(주요 의사결정)은 기존대로 공통 `docs/adr/`에 ADR로 기록합니다.\n\n```\nREADME.md (레포 루트)          ← 레벨 0\ndocs/space-view/README.md      ← 하이레벨 (폴더의 진입점)\ndocs/space-view/*.md           ← 미드레벨 (README를 제외한 나머지)\ndocs/adr/NNNN-*.md             ← 로우레벨 (기능 무관, 번호순 공통 보관)\n```\n\n## 왜\n\n지금 `docs/design/`의 문서는 사람이 읽기엔 양이 많고(2,800줄) product/features/architecture로 쪼개져 있어, 읽히지 않고 인지부채만 쌓이는 문제가 있었습니다. 사람이 보기 좋은 짧은 문서 기반으로 바꿔서 **문서 + 채팅만으로 소통 가능한** 상태를 목표로 합니다.\n\n## 변경 상세\n\n- `docs/design/frontend-viz/` 5개 파일 삭제 — 상세 원문은 git 히스토리 참조 (`git show ae6a65e -- 'docs/design/frontend-viz/*'`), archive 폴더는 만들지 않음\n- 외부 참조 수정: `docs/README.md` 1곳, `collab-space/05-contracts.md` 3곳 (링크 대상만 변경, 내용 무변경)\n- ADR 0002의 `design/frontend-viz` 언급은 시점 기록물이라 수정하지 않음\n- `contracts/`는 건드리지 않음 — 계약은 기존 원칙대로 파일이 정답\n\n## 다음 단계 (별도 PR)\n\n- 팀 합의 후 `overview-mentor/` → `docs/coaching/`, `collab-space/` → `docs/agent-space/` 이관\n- CLAUDE.md에 레벨 구조·문서 작성 원칙(분량 상한, 고아 문서 금지 등) 반영\n\n🤖 Generated with [Claude Code](https://claude.com/claude-code)",
  "createdAt": "2026-07-13T14:54:07Z",
  "headRefName": "docs/space-view-restructure",
  "mergedAt": null,
  "number": 5,
  "state": "OPEN",
  "title": "docs(space-view): restructure frontend-viz docs into leveled space-view docs",
  "updatedAt": "2026-07-13T14:56:38Z",
  "url": "https://github.com/dev-team-404/space-a/pull/5"
 },
 {
  "author": {
   "id": "MDQ6VXNlcjkzMTE5OTA=",
   "is_bot": false,
   "login": "msaltnet",
   "name": "Jeong Seongmoon"
  },
  "body": "ADR 0001이 \"다음에 기록할 결정\"으로 예고했던 두 건을 기록합니다.\n\n## ADR 0002 — 레포 구성 (제안, 승인 필요)\n\n**\"모노레포 vs 멀티레포\"를 이론적으로 고를 상황이 아니었습니다.** 이미 사실이 정해져 있습니다.\n\n| Pillar | 현재 위치 |\n|---|---|\n| 1. Agent Mentor | **이 저장소 밖** — Rust/Tauri 데스크톱 앱, 별도 저장소에서 개발 중 |\n| 2. Space A Hub | 이 저장소 (코드는 아직 없음) |\n| 3. 시각화 | 이 저장소 (`prototype/frontend-viz/`) |\n\n그래서 진짜 질문은 **\"이미 흩어진 것을 어떻게 정리할 것이냐\"**입니다.\n\n### 결정: 계약 중심 하이브리드\n\n**레포가 몇 개인지는 중요하지 않습니다. 계약이 한 곳에 있느냐가 중요합니다.**\n\n- **`contracts/`가 유일한 결합점** — 세 컴포넌트는 코드를 공유하지 않고, 스키마만 참조합니다\n- **Pillar 1은 밖에 유지** — 스택(Rust/Tauri)도 배포 주기(앱 릴리즈)도 다르고, 결합점은 C1 하나뿐입니다. 되돌리는 비용이 얻는 것보다 큽니다\n- **Pillar 2/3은 같이 유지** — 계약을 바꾸는 PR에서 **소비자가 깨지는지 바로 보이는 것**이 실질적 안전망입니다\n- **설계 문서는 전부 이 저장소에** — 세 축을 한자리에서 볼 수 있어야 합니다\n\n### 감수하는 것\n\n**Pillar 1은 계약 변경을 CI로 못 잡습니다.** 저장소가 달라서입니다. `contracts/`의 변경 정책(추가만 허용)이 1차 방어선이고, C1을 깨는 변경은 버전을 올려 별도 고지해야 합니다.\n\n## ADR 0003 — 로컬 LLM 클러스터는 공용 인프라 (채택)\n\n합의된 내용을 기록합니다.\n\n**클러스터를 Pillar 2 소유로 두면 안 되는 이유:** Pillar 1(코칭)도 로그 파싱에 로컬 LLM이 필요합니다. Pillar 2 소유가 되면 **Pillar 2의 배포가 Pillar 1의 기능을 멈춥니다.**\n\n- 클러스터는 **어느 Pillar에도 속하지 않음.** Pillar 1·2는 소비자일 뿐\n- 각 Pillar가 아는 건 **OpenAI 호환 엔드포인트 주소 하나** → 클러스터가 없어도 개발이 안 멈춤\n- 자원 경쟁은 **용도별 모델명**으로 분리 (`space-a-ingest` / `space-a-coach`)\n- 분산 추론(Exo)이 아니라 **로드밸런싱** — 백오피스 작업엔 거대 모델 1개보다 빠른 모델 여러 개\n\n### ⚠️ 미결 리스크로 기록한 것\n\n- **`OLLAMA_HOST=0.0.0.0`은 인증 없이 GPU를 여는 것입니다.** 최소한 방화벽으로 게이트웨이 IP만 허용해야 합니다 (인프라 담당 협의 필요)\n- **운영 주체 미정** — 공용 인프라라 누구의 담당도 아닙니다\n- 게이트웨이가 SPOF (해커톤 규모에선 감수)\n\n## 확인 요청\n\n- [ ] **ADR 0002 승인** — 특히 \"Pillar 1을 별도 저장소로 유지\"에 이견 없는지\n- [ ] 클러스터 **운영 주체** 지정\n\n> PR #3(계약 v2)과 독립적입니다. 파일이 겹치지 않습니다.\n\n🤖 Generated with [Claude Code](https://claude.com/claude-code)",
  "createdAt": "2026-07-12T13:51:57Z",
  "headRefName": "docs/adr-repo-structure",
  "mergedAt": "2026-07-13T08:53:40Z",
  "number": 4,
  "state": "MERGED",
  "title": "docs(adr): record repo structure and shared LLM cluster decisions",
  "updatedAt": "2026-07-13T08:53:40Z",
  "url": "https://github.com/dev-team-404/space-a/pull/4"
 },
 {
  "author": {
   "id": "MDQ6VXNlcjkzMTE5OTA=",
   "is_bot": false,
   "login": "msaltnet",
   "name": "Jeong Seongmoon"
  },
  "body": "두 가지가 들어 있습니다. **①이 이 프로젝트의 핵심 차별점**이고, ②는 그 전제가 되는 계약 정합입니다.\n\n---\n\n# ① 자기 진화하는 공간 — Confluence와 갈라지는 지점\n\n## 문제: 내 초안은 Confluence를 다시 만들고 있었다\n\n첫 governance 초안은 무활동 30일/90일, 예산 80%/100%, trust 임계치를 **전부 상수로 고정**했습니다.\n사람이 숫자를 정해두고 시스템이 집행하는 구조 — **관리자 설정 화면과 다를 게 없습니다.**\n\n## 핵심 명제\n\n> **Confluence는 공간의 *구조*가 고정되어 있고, 사용자가 거기 맞춥니다.**\n> 스페이스·페이지·라벨 스키마는 개발자가 정했고 10년째 그대로입니다.\n> 사용 패턴이 아무리 바뀌어도 **사람이 관리자 화면에 들어가야만** 바뀝니다.\n> 대부분의 조직에서 아무도 안 들어갑니다. **그래서 위키는 죽습니다.**\n>\n> **Space A는 공간이 *스스로 자기 구조를 다시 짭니다*.**\n\n자주 팀 경계를 넘는 지식은 허브로 승격되고, 안 쓰는 분류는 접히고, 새 클러스터가 보이면 방을 제안합니다.\n**아무도 관리자 화면에 안 들어갔는데 정보 구조가 바뀝니다.**\n\n**한 줄로:** Confluence는 **사람이 관리해야 사는** 시스템이고, Space A는 **안 관리해도 스스로 자라는** 시스템입니다.\n\n## 결정적 제약 — 사용성은 유지된다\n\n```\n에이전트: search_knowledge(\"인증서 오류\")   ← 영원히 안 바뀜 (C1 동결)\n                    ↓\n        그 아래에서 공간이 스스로 재편  ← 매달 달라짐\n```\n\n**C1의 핵심 Tool을 얼리는 것**이 \"사용성 유지\"의 실체입니다.\n\n## 통제 모델 — 사람은 울타리를 치고, 공간은 안에서 뛴다\n\n변경을 **되돌리기 난이도**로 등급화하고, **\"어떤 등급이 자동/승인인지\"의 매핑을 사람이 정합니다.**\n\n| 등급 | 예 | 통제 |\n|---|---|---|\n| **L0** 가역·무해 | 순위 조정 | 즉시 자동 |\n| **L1** 가역·유의미 | 파라미터 튜닝, 허브 승격, 분류 재편 | **자동 + 기록 + 되돌리기 가능** |\n| **L2** 영향 큼 | 방 제안·병합 | **사람 승인** |\n| **L3** 파괴적 | 지식 삭제, 권한 구조 변경 | **금지 — 공간이 못 함** |\n\n```yaml\n# 진화 정책 — 사람이 정하고, 공간은 읽되 고칠 수 없다\nevolution_policy:\n  hub_promotion:      auto\n  space_proposal:     approval\n  knowledge_deletion: forbidden\n  # 신뢰가 쌓이면 사람이 등급을 낮출 수 있다\n```\n\n**\"규칙 자체 생성\"을 배제한 이유:** 공간이 자기 통제 정책까지 바꾸면 통제가 무의미해집니다.\n\"나는 이제 승인 없이 방을 지울 수 있다\"고 스스로 정하는 순간 안전망이 사라집니다.\n\n## 진화의 재료는 이미 C1이 만들고 있다\n\n**추가 계측이 필요 없습니다.** `cite_knowledge`의 출발지↔도착지가 곧 \"이 지식이 팀 경계를 몇 번 넘었나\"입니다.\n\n## 안전장치\n\n자기 진화의 진짜 위험은 **잘못된 방향으로 빠르게 수렴하는 것**입니다.\n\n| 위험 | 대책 |\n|---|---|\n| 부익부 — 뜬 지식만 계속 노출 | 신규 지식에 **탐색 보너스** |\n| 다수결의 폭정 — 큰 팀 방식이 표준화 | 허브 승격은 **인용 팀 수**로 판단 (횟수 아님) |\n| 진화 폭주 | **주당 변경 예산** 상한 |\n| 되돌림 무시 | 되돌려진 변경은 **쿨다운** |\n\n---\n\n# ② C1/C2를 Pillar 3 데이터 모델과 정합 (v2)\n\n**v1 계약이 틀렸습니다.** Pillar 3의 데이터 계약과 대조하기 전에 확정했는데, 대조해보니 **권한 모델의 기반부터 어긋나 있었습니다.**\n\n| # | v1 | v2 | 이유 |\n|---|---|---|---|\n| 1 | `team` **문자열 하나** | **`Space` 1급 개념**, 토큰 클레임 `spaces[]` | **한 사람이 여러 Space에 속합니다.** 단일값으로는 표현 불가 |\n| 2 | `private\\|team\\|public` | `org\\|space` | Pillar 3와 값 통일 |\n| 3 | `report_issue` 1회 | `open_issue` → `cite_knowledge` → `resolve_issue` | Pillar 3 UI가 **진행 중 이슈 타임라인**을 그림 |\n| 4 | 재사용 = 그래프 엣지 | **`ReuseEvent` 1급 이벤트** | 북극성 지표를 엣지에 묻어둔 건 실수 |\n\n## 🔒 에이전트 경유 권한 우회 (v1의 실제 구멍)\n\n> \"사람이 UI에서 못 보는 것을 자기 에이전트에게 시켜서 볼 수 있으면 유리벽이 무의미해진다.\" — Pillar 3 설계\n\nv1은 C1이 `team` 필터, C2가 Space 멤버십 — **두 모델이 달라서 우회가 가능**했습니다.\nv2는 **C1과 C2가 하나의 규칙**을 씁니다: `visibility: org` 지식 + 자기가 속한 Space의 데이터.\n\n**집행은 서버가 합니다.** 프론트는 아무것도 숨기지 않습니다.\n\n```sh\ndiff contracts/fixtures/space-detail-member.json \\\n     contracts/fixtures/space-detail-guest.json   # 유리벽이 무엇을 가리는지\n```\n\n## C2는 Pillar 3의 렌더링 방식을 결정하지 않는다\n\n초안에서 제가 선을 넘었습니다. `\"the frontend must NOT assemble this\"`라고 박아뒀는데,\nPillar 3 문서는 그 질문(**Q4 — 서사 번역 파이프라인**)을 **\"가장 중요한 열린 질문\"**으로 남겨둔 상태였습니다.\n**그쪽이 안 정했다고 명시한 걸 제 계약이 몰래 닫아버린 것**이라 철회했습니다.\n\nC2는 이제 **구조화 필드와 서사 필드를 둘 다 제공하고 빠집니다.** 어느 쪽을 쓸지는 Pillar 3이 정합니다.\n\n---\n\n## 확인 요청\n\n- [ ] **@kimmykim-jy (Pillar 3)** — ①의 진화 설계, 그리고 ②에서 제가 그쪽 계약을 맞게 읽었는지\n- [ ] **Pillar 1 담당** — 이슈 생애주기가 **3개 호출**로 나뉘는데 에이전트 동선에 부담인지\n\n## 미결정 (팀 논의)\n\n- [ ] **울타리의 초기 위치** — 무엇을 auto로 시작하고 무엇을 approval로 둘 것인가\n- [ ] 파라미터 clamp 범위 / 주당 변경 예산 / 허브 승격 임계치 (몇 개 팀?)\n\n🤖 Generated with [Claude Code](https://claude.com/claude-code)",
  "createdAt": "2026-07-12T13:19:42Z",
  "headRefName": "docs/contracts-v2-align",
  "mergedAt": "2026-07-13T08:52:55Z",
  "number": 3,
  "state": "MERGED",
  "title": "docs: self-evolving space + align C1/C2 with Pillar 3 (v2)",
  "updatedAt": "2026-07-13T08:52:55Z",
  "url": "https://github.com/dev-team-404/space-a/pull/3"
 },
 {
  "author": {
   "id": "MDQ6VXNlcjkzMTE5OTA=",
   "is_bot": false,
   "login": "msaltnet",
   "name": "Jeong Seongmoon"
  },
  "body": "SPACE-A 3대 축 중 **Pillar 2 — 에이전트 자율 협업 공간**의 설계 문서 묶음을 추가합니다.\n`overview-mentor/`(Pillar 1)와 동일한 구조를 따랐습니다.\n\n## 문서 구성\n\n| 문서 | 내용 |\n|---|---|\n| `README.md` | 30초 요약 + 문서 지도 |\n| `01-product.md` | 문제 정의, 시장 조사(Mem0/AutoGen/Dify 대비 포지셔닝), MCP vs Agent Skills, 설계 원칙 |\n| `02-features.md` | Knowledge Contract 스키마, 하이브리드 그래프 RAG, 심야 압축, Skill 승격, 안전장치 |\n| `03-architecture.md` | 컴포넌트 경계, 외부 계약(C1~C3), 포트/어댑터 구조, 공용 GPU 클러스터 |\n| `04-roadmap.md` | 구현 순서, 데모 시나리오, 미결정 사항, 리스크 |\n\n## 주요 결정\n\n- **MCP 서버로 구현** (Agent Skills 배포가 아니라) — 제로 세팅·중앙 보안·플러그앤플레이.\n- **GPU 클러스터는 Pillar 2 소유가 아님** — Pillar 1(코칭)도 로컬 LLM이 필요하므로 **공용 인프라**로 분리. Pillar 2는 소비자일 뿐.\n- **Pillar 3은 Graph DB에 직접 붙지 않음** — 읽기 전용 REST 계약만 제공. 안 그러면 그래프 스키마가 프론트의 공개 API가 되어 진화가 멈춤.\n- **포트/어댑터 구조** — Core가 인터페이스에만 의존하므로 **Vector/Graph DB 선택을 미뤄도** 개발이 진행됨.\n\n## 팀에 요청하는 것 (병렬화의 전제)\n\n다음 세 가지가 정해져야 세 컴포넌트가 동시에 작업 가능합니다.\n\n- [ ] **C1: MCP Tool 시그니처** 확정 (`search_knowledge`, `report_issue` …)\n- [ ] **C2: 시각화용 읽기 REST 스키마** 확정 → Pillar 3에 픽스처 선전달\n- [ ] **로컬 LLM 클러스터 소유권 합의** (공용 인프라로 분리)\n\n레포 구성은 되돌리기 어려운 결정이라 확정 시 별도 ADR로 남길 예정입니다.\n\n🤖 Generated with [Claude Code](https://claude.com/claude-code)",
  "createdAt": "2026-07-12T11:35:14Z",
  "headRefName": "docs/collab-space-design",
  "mergedAt": "2026-07-12T13:06:44Z",
  "number": 2,
  "state": "MERGED",
  "title": "docs(design): add collab-space design docs for agent collaboration hub",
  "updatedAt": "2026-07-12T13:06:44Z",
  "url": "https://github.com/dev-team-404/space-a/pull/2"
 }
];
const LIVE_NARRATIVES = {
 "2": {
  "key": "2:docs(design): add collab-space design docs for agent collaboration hub",
  "label": "협업 공간 설계",
  "title": "에이전트 협업 허브를 위한 협업 공간 설계 추가",
  "summary": "에이전트들이 협업하는 공간의 구조와 기능을 설계 문서로 정의합니다."
 },
 "3": {
  "key": "3:docs: self-evolving space + align C1/C2 with Pillar 3 (v2)",
  "label": "자체 진화 공간 정렬",
  "title": "자체 진화 공간과 Pillar 3 계약 정렬 (v2)",
  "summary": "자체 진화하는 공간의 개념과 C1/C2를 Pillar 3과 맞춥니다."
 },
 "4": {
  "key": "4:docs(adr): record repo structure and shared LLM cluster decisions",
  "label": "ADR 기록",
  "title": "저장소 구조와 LLM 클러스터 결정 사항 기록",
  "summary": "주요 아키텍처 결정인 저장소 구조와 공유 LLM 클러스터를 ADR로 남깁니다."
 },
 "5": {
  "key": "5:docs(space-view): restructure frontend-viz docs into leveled space-view docs",
  "label": "공간 뷰 문서 재편",
  "title": "프론트엔드 시각화 문서를 단계별 공간 뷰로 재구성",
  "summary": "프론트엔드 시각화 문서를 더 체계적인 공간 뷰 구조로 정리합니다."
 },
 "6": {
  "key": "6:feat(frontend): reflect real GitHub PR activity in prototype as live space",
  "label": "실시간 PR 활동 반영",
  "title": "프로토타입 스페이스에 실제 PR 활동 표시",
  "summary": "레포의 열린 PR을 스냅숏으로 스페이스 타임라인에 실시간 반영"
 }
};
const LIVE_FETCHED_AT = '07-14 00:22';
