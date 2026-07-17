"""인메모리 Store 어댑터.

MVP용. ports & adapters 구조라, 나중에 실제 DB 어댑터로 교체해도 core는 안 바뀐다.
id는 접두어별 순번(agt_1, iss_1, page_1, ...)으로 발급해 테스트가 결정론적이다.
"""

from ..core.models import Agent, Issue, Page, ReuseEvent, Space
from ..core.ports import Store


class InMemoryStore(Store):
    def __init__(self) -> None:
        self._spaces: dict[str, Space] = {}
        self._agents: dict[str, Agent] = {}
        self._tokens: dict[str, str] = {}
        self._issues: dict[str, Issue] = {}
        self._pages: dict[str, Page] = {}
        self._reuse: list[ReuseEvent] = []
        self._seq: dict[str, int] = {}

    def new_id(self, prefix: str) -> str:
        self._seq[prefix] = self._seq.get(prefix, 0) + 1
        return f"{prefix}_{self._seq[prefix]}"

    def new_token(self) -> str:
        return self.new_id("tok")

    def add_space(self, space: Space) -> None:
        self._spaces[space.id] = space

    def get_space(self, space_id: str) -> Space | None:
        return self._spaces.get(space_id)

    def all_spaces(self) -> list[Space]:
        return list(self._spaces.values())

    def add_agent(self, agent: Agent) -> None:
        self._agents[agent.id] = agent

    def bind_token(self, token: str, agent_id: str) -> None:
        self._tokens[token] = agent_id

    def agent_for_token(self, token: str) -> Agent | None:
        agent_id = self._tokens.get(token)
        return self._agents.get(agent_id) if agent_id else None

    def get_agent(self, agent_id: str) -> Agent | None:
        return self._agents.get(agent_id)

    def save_agent(self, agent: Agent) -> None:
        self._agents[agent.id] = agent

    def all_agents(self) -> list[Agent]:
        return list(self._agents.values())

    def revoke_tokens(self, agent_id: str) -> None:
        self._tokens = {t: a for t, a in self._tokens.items() if a != agent_id}

    def add_issue(self, issue: Issue) -> None:
        self._issues[issue.id] = issue

    def get_issue(self, issue_id: str) -> Issue | None:
        return self._issues.get(issue_id)

    def save_issue(self, issue: Issue) -> None:
        self._issues[issue.id] = issue

    def all_issues(self) -> list[Issue]:
        return list(self._issues.values())

    def add_page(self, page: Page) -> None:
        self._pages[page.id] = page

    def get_page(self, page_id: str) -> Page | None:
        return self._pages.get(page_id)

    def save_page(self, page: Page) -> None:
        self._pages[page.id] = page

    def all_pages(self) -> list[Page]:
        return list(self._pages.values())

    def pages_in_space(self, space_id: str) -> list[Page]:
        return [p for p in self._pages.values() if p.space_id == space_id]

    def add_reuse_event(self, event: ReuseEvent) -> None:
        self._reuse.append(event)
