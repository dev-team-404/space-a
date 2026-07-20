# ADR 0014: 소셜 공간 도메인 명칭을 Life로 통일한다

- 상태: 채택
- 날짜: 2026-07-20
- 대체: ADR 0012의 Life Server·Life API 명칭 유지 결정

## 배경

소셜 공간 컴포넌트의 물리 경로와 제품 개념은 `a-hub/life`이지만 구현에는 Life Server,
`life_server`, Life API, `Life*` 타입과 `life*` 설정이 남아 있다. 같은 컴포넌트를 Life와
Life이라는 두 이름으로 부르면서 실행 단위, API 계약, 클라이언트 설정의 경계가 불분명하다.

## 결정

- 컴포넌트와 실행 단위는 **Life Server**로 부른다.
- Python 패키지·이미지·컨테이너·Compose 서비스·환경 변수는 `life_server`,
  `space-a-life-server`, `life-server`, `LIFE_SERVER_*`를 사용한다.
- HTTP 자원 경로는 `/life`에서 `/life`로 바꾸고 식별자는 `life_id`로 통일한다.
- 코드의 공개 타입·함수·모듈·UI 컴포넌트·설정 키에서 `Life`/`life`을
  `Life`/`life`로 바꾼다.
- 활성 설계 문서와 계약은 Life 명칭을 사용한다. 채택된 과거 ADR과 보관 문서는 당시 결정을
  보존하며, 이 ADR의 대체 관계로 변경 이력을 설명한다.
- 이전 이름과 API 경로의 호환 별칭은 두지 않는다. 서버와 소비자를 한 변경으로 전환한다.

## 결과

- 저장소 경로, 서비스 이름, API, 클라이언트가 하나의 Life 용어를 공유한다.
- `/life` 소비자와 `LIFE_SERVER_*` 배포 설정은 새 계약으로 마이그레이션해야 한다.
- OCI 배포 시 이미지·컨테이너·볼륨·환경 변수 변경을 함께 적용해야 한다.

## 검증

- Life Server 테스트와 A-Mate/A-Lens 테스트·빌드를 실행한다.
- 새 Compose 이미지로 서버를 기동해 `/healthz`와 `/life` 계약을 호출한다.
- 활성 코드·계약·문서와 파일 경로에서 이전 Life 명칭이 남지 않았는지 검색한다.
