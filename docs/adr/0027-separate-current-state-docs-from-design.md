# ADR 0027: 현행 상태 문서를 `docs/architecture/`로 분리한다

- 상태: 채택
- 날짜: 2026-08-03

## 배경

[ADR 0013](./0013-docs-lifecycle-and-archive.md)은 **작업 문서**(spec / plan / kickoff)의
수명주기를 정했다 — "위치가 곧 상태", 완료되면 `docs/archive/`로 미러.

그런데 정해지지 않은 종류가 하나 더 있었다. **"지금 이렇게 되어 있다"를 기술하는 현행 문서**다.
제품 소개·기능 카탈로그·아키텍처·연혁·빌드 가이드가 여기 해당한다.

이들이 갈 곳이 없어 `docs/design/<component>/` 아래에 작업 문서와 섞여 있었다.

```
docs/design/a-mate/
├── 01-product.md          ┐
├── 02-features.md         │  현행 — 코드가 바뀌면 같이 고쳐야 함
├── 03-architecture.md     │
├── 04-history-and-roadmap.md
├── build-and-run.md       ┘
├── specs/                 ┐  시점 — 합의 시점의 기록, 소급 수정 금지
├── plans/                 │
└── brainstorming/         ┘
```

두 종류의 수명이 정반대인데 한 폴더에 있었다. 실제로 다음 문제가 나타났다.

- **낡아도 아무도 모른다.** `03-architecture.md`가 2026-07-10 실측에서 멈춰 있었다.
  그 사이 코칭 규칙 12종 → 3종, Tauri 커맨드 19개 → 77개, 팀 지식 허브·미니홈피·인정 루프·
  콘텐츠 큐레이션이 들어왔지만 문서는 그대로였다. CLAUDE.md가 design을 "설계 단계 작업물"로
  정의했기 때문에, 읽는 쪽도 쓰는 쪽도 이 파일을 "갱신 대상"으로 보지 않았다.
- **어디에 쓸지 매번 헷갈린다.** 현행 문서의 자리가 명문화돼 있지 않아, 작성자마다 다른 곳에 만든다.
- **중복이 생긴다.** 2026-08-03에 `docs/architecture/a-mate.md`가 별도로 만들어지면서
  아키텍처 문서가 두 개가 됐고, 한쪽을 리다이렉트 스텁으로 만드는 임시 조치가 들어갔다.
  스텁은 탐색을 한 단계 늘릴 뿐 문제를 풀지 않는다.

## 결정

- **`docs/architecture/<component>/`를 현행 문서의 자리로 신설한다.**
  제품·기능·구조·연혁·빌드 등 "지금 상태"를 기술하는 문서는 전부 여기 둔다.
- **`docs/design/<component>/`에는 설계 안과 작업 문서만 남긴다** (`specs/`·`plans/`·`brainstorming/`).
- **판단 기준은 한 줄이다.** "코드가 바뀌면 이 문서도 고쳐야 하나?"
  그렇다 → `architecture/`. 아니다(그때의 결정·계획 기록) → `design/`·`adr/`.
- **현행 문서는 같은 PR에서 갱신한다.** 코드를 바꾸면서 사실이 달라지면 그 PR 안에서 고친다.
  ADR 0013의 아카이브 DoD와 같은 층위의 규율이다.
- **작업 문서는 소급 수정하지 않는다.** spec·plan·ADR이 현재와 달라졌으면 `architecture/`를
  고치지, 옛 문서를 고쳐 쓰지 않는다.
- **같은 내용을 두 곳에 두지 않는다.** 리다이렉트 스텁도 두지 않는다 —
  참조하는 쪽의 링크를 직접 고친다.

### 적용 (a-mate)

```
docs/architecture/a-mate/         ← 현행
├── README.md                     문서 묶음 안내
├── 01-product.md                 제품 비전·포지셔닝·설계 원칙
├── 02-features.md                기능 카탈로그
├── 03-architecture.md            코드 구조
├── 04-history-and-roadmap.md     연혁·교훈·로드맵
├── build-and-run.md              빌드·실행·트러블슈팅
└── assets/

docs/design/a-mate/               ← 시점
├── specs/  plans/  brainstorming/
```

a-hub·a-lens는 같은 문제를 갖고 있지만 담당자가 다르므로 이번에 옮기지 않는다.
각 담당자가 필요할 때 같은 규칙으로 옮긴다.

## 결과

- 현행 문서가 한 곳에 모여 **"낡았는지"를 판단할 수 있는 단위**가 생긴다.
- `docs/design/`이 본래 정의("설계 안")와 일치하게 되어, 그 안의 문서를 소급 수정하지 않는
  규율이 자연스러워진다.
- 문서 이동 비용이 발생한다 — 이번 이동에서 참조 갱신 대상은 8개 파일이었고, 상대링크
  전수 검사로 깨짐 0건을 확인했다.
- 컴포넌트별로 `architecture/`가 하나씩 더 생기므로, 컴포넌트가 늘면 디렉터리도 는다.
  대안(각 컴포넌트 폴더 안에 두기)은 코드 저장소와 문서가 섞여 기각했다.

## 검증

- `docs/design/a-mate/`에 `specs/`·`plans/`·`brainstorming/`만 남는다.
- 레포 전체에서 옛 경로(`docs/design/a-mate/{01,02,03,04,README,build-and-run}`)를 가리키는
  참조가 0건이다. 단, `specs/`·`plans/` 내부의 옛 경로 언급은 **시점 기록이라 그대로 둔다**.
- CLAUDE.md 문서 구조 표에 `architecture/`가 성격("현행")과 함께 명시된다.
