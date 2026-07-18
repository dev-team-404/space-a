# ADR 0012: Room Server를 `a-hub/life`에 배치한다

- 상태: 채택
- 날짜: 2026-07-19

## 배경

목표 아키텍처는 A-Hub를 업무 협업인 `work`와 소셜 공간인 `life` 두 축으로 정의하고,
개인 방·입장자·위치·인테리어를 관리하는 Room Server를 Life 서비스로 본다. 그러나 저장소에서는
`a-hub/life`가 플레이스홀더인 반면 실제 구현이 최상위 `room-server`에 있어 개념 구조와 물리 구조가
어긋나 있었다.

이 불일치는 다음 문제를 만든다.

- A-Hub Life와 Room Server가 서로 다른 컴포넌트처럼 보인다.
- 실행·테스트·Docker 문서가 두 위치를 번갈아 가리킨다.
- A-Hub의 `work`와 `life`를 한 위치에서 찾을 수 없다.

## 결정

- 최상위 `room-server/`의 구현 전체를 `a-hub/life/`로 이동한다.
- 기존 `a-hub/life/README.md` 플레이스홀더는 실제 Room Server README로 대체한다.
- Python 패키지명 `room_server`, 배포 이미지·컨테이너명 `space-a-room-server`, 환경 변수
  `ROOM_SERVER_DB`, 호스트 포트 `8001`, Rooms API 계약은 유지한다.
- `a-hub/work`와 `a-hub/life`는 같은 상위 폴더에 있지만 계속 별도 Python 프로젝트,
  Docker Compose 프로젝트, 데이터 저장소, 서버 프로세스로 실행한다.
- 저장소 문서와 실행 예시는 `a-hub/life`를 정식 경로로 사용한다. 최상위 `room-server` 호환
  심볼릭 링크나 복제본은 두지 않는다.

## 결과

- A-Hub의 물리 구조가 `work/`와 `life/`라는 개념 구조와 일치한다.
- 기존 클라이언트와 API 소비자는 URL·포트·프로토콜이 바뀌지 않으므로 수정 없이 동작한다.
- 로컬 실행과 CI는 작업 디렉터리 또는 Compose 파일 경로를 `a-hub/life`로 바꿔야 한다.
- 서버 코드는 다른 폴더로 이동하지만 A-Hub Work에 통합된 단일 프로세스가 되지는 않는다.

## 검증

- `a-hub/life`에서 Python 테스트 전체를 실행한다.
- `a-hub/life/docker-compose.yml`로 이미지를 빌드하고 컨테이너를 기동한다.
- 호스트 포트 `8001`의 `/healthz`와 `/capabilities` 응답을 확인한다.
- 저장소에서 최상위 `room-server/` 경로를 참조하는 문서·설정이 남지 않았는지 검색한다.
