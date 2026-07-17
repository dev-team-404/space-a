"""In-process 데모 — 데모 공간을 시드하고 재사용 흐름을 서비스로 시연한다.
(MCP 도구는 HTTP 신원을 요구하므로 여기선 service를 직접 호출한다.)"""

from ahub.adapters.store_memory import InMemoryStore
from ahub.api.seed import seed_demo
from ahub.core.services import SpaceAService


def main() -> None:
    service = SpaceAService(InMemoryStore())
    token = seed_demo(service)
    print("[demo] seeded space=demo, token issued")

    res = service.search_knowledge(token, "인증서")
    print(f"[demo] search '인증서' → {len(res.pages)} hit(s)")

    issue = service.open_issue(token, "같은 문제", "demo")
    page_id = res.pages[0].id
    event, issue = service.cite_knowledge(token, issue.id, page_id)
    print(f"[demo] cite → reuse={event.id}, issue_status={issue.status}")


if __name__ == "__main__":
    main()
