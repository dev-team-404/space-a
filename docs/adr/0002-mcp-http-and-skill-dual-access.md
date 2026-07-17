# 0002. MCP HTTP 전송 전환 + Skill(REST) 이중 입구

- 상태: 채택(Accepted)
- 날짜: 2026-07-17

## 배경

a-hub/work의 MCP는 stdio + 단일 토큰(SPACE_A_TOKEN)으로만 구현되어, 계약 C1(§3.1)이
"기본"으로 정한 HTTP + per-request Bearer 경로가 비어 있었다. stdio 방식은 클라이언트에
Python과 이 코드가 있어야 붙을 수 있어 "Python 없는 환경에서도 붙는다"는 MCP의 이점을
스스로 무효화했다. 또 MCP를 못 붙이는 환경(Claude Code 등)에서 허브 기능에 접근할
방법이 없었다.

## 결정

1. MCP 전송을 stdio → **Streamable HTTP**로 전환한다. 신원은 요청의 Authorization
   헤더(per-request Bearer)에서 온다. stdio 진입점과 기동 시 데모 시드는 제거한다.
2. REST와 MCP를 **한 프로세스**에서 서빙한다: `create_app(mount_mcp=True)`가 `/mcp`를
   마운트한다. 둘은 같은 SpaceAService·스토어·Bearer 규칙을 공유한다.
3. **Skill 패키지**(`a-hub/work/skills/space-a-hub/`)로 비-MCP 환경이 동일한 REST
   엔드포인트를 호출하게 한다. 범위는 MCP 도구 6종과 동일하다.
4. **Lambda(서버리스, 개발용)는 REST만** 서빙한다(`mount_mcp=False`). MCP는 상주
   컨테이너 전용 — Streamable HTTP의 상주/스트리밍이 API Gateway+Lambda와 맞지 않는다.

## 결과

- MCP·REST·Skill 세 접근이 같은 도메인 로직·데이터를 공유한다.
- `mcp`가 서버 기본 의존성이 되지만, Lambda는 지연 import로 번들에서 제외된다.
- 계약 C1이 구현으로 완성된다. 클라이언트는 HTTP URL만 알면 붙으므로 Python이
  클라이언트에 필요 없다.
