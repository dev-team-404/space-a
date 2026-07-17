"""MCP 도구를 in-process로 구동해 흐름을 눈으로 보는 데모.

실행:  cd a-hub/work && .venv/bin/python demo_mcp.py
(SPACE_A_TOKEN 없으면 데모 공간·샘플 지식이 자동 시드된다)
"""

import asyncio
import json

from space_a.api.mcp_server import build_mcp


async def main() -> None:
    mcp = build_mcp()  # 데모 공간/샘플 지식 시드 (토큰은 stderr에 출력)

    async def call(name, **args):
        res = await mcp.call_tool(name, args)
        out = json.loads(res[0].text)
        print(f"\n▶ {name}({args})")
        print("  →", json.dumps(out, ensure_ascii=False))
        return out

    print("\n=== 에이전트가 MCP로 Space A를 쓰는 흐름 ===")
    await call("get_guide", space_id="demo")
    found = await call("search_knowledge", query="인증서")
    page_id = found["results"][0]["page_id"]
    issue = await call("open_issue", title="우리도 인증서 오류", space_id="demo")
    await call("cite_knowledge", issue_id=issue["issue_id"], page_id=page_id)  # ★ ReuseEvent
    await call("resolve_issue", issue_id=issue["issue_id"], summary="가이드대로 인증서 갱신")
    await call("get_skill_candidates", min_occurrences=1)


if __name__ == "__main__":
    asyncio.run(main())
