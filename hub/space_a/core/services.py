"""Space A 도메인 서비스.

관리(create_space, register_agent), 이슈 생애주기(open_issue, resolve_issue),
검색·재사용(search_knowledge, cite_knowledge).
권한 규칙: 소속(spaces)은 토큰에서 유도하며, 요청 인자로 주장할 수 없다.
"""

from . import errors
from .models import Agent, Issue, Page, ReuseEvent, SearchResult, Space
from .ports import Store


class SpaceAService:
    def __init__(self, store: Store):
        self.store = store

    # --- 관리 (control plane) ---

    def create_space(self, space_id: str, name: str) -> Space:
        space = Space(id=space_id, name=name)
        self.store.add_space(space)
        return space

    def register_agent(self, name: str, space_id: str) -> tuple[Agent, str]:
        if self.store.get_space(space_id) is None:
            raise errors.NotFound(f"space '{space_id}' not found")
        agent = Agent(id=self.store.new_id("agt"), name=name, spaces=[space_id])
        self.store.add_agent(agent)
        token = self.store.new_token()
        self.store.bind_token(token, agent.id)
        return agent, token

    # --- 이슈 생애주기 ---

    def open_issue(self, token: str, title: str, space_id: str) -> Issue:
        agent = self._authed_agent(token)
        if space_id not in agent.spaces:
            raise errors.Forbidden(f"not a member of space '{space_id}'")
        issue = Issue(
            id=self.store.new_id("iss"),
            space_id=space_id,
            title=title,
            opened_by=agent.id,
        )
        self.store.add_issue(issue)
        return issue

    def resolve_issue(
        self,
        token: str,
        issue_id: str,
        summary: str,
        steps: list[str] | None = None,
        publish_knowledge: bool = True,
        visibility: str = "org",
    ) -> tuple[Issue, Page | None]:
        agent = self._authed_agent(token)
        issue = self.store.get_issue(issue_id)
        if issue is None:
            raise errors.NotFound(f"issue '{issue_id}' not found")
        if issue.space_id not in agent.spaces:
            raise errors.Forbidden("issue belongs to a space you are not a member of")

        issue.status = "resolved"
        self.store.save_issue(issue)

        page: Page | None = None
        if publish_knowledge:
            page = Page(
                id=self.store.new_id("page"),
                space_id=issue.space_id,
                title=summary,
                source="issue-derived",
                visibility=visibility,
                issue_id=issue.id,
                steps=steps or [],
            )
            self.store.add_page(page)
        return issue, page

    # --- 검색 · 재사용 ---

    def search_knowledge(
        self, token: str, query: str, space_id: str | None = None, limit: int = 3
    ) -> SearchResult:
        agent = self._authed_agent(token)
        scope = [p for p in self.store.all_pages() if self._visible(p, agent)]
        if space_id is not None:
            scope = [p for p in scope if p.space_id == space_id]
        terms = [t for t in query.lower().split() if t]
        hits = [
            p for p in scope if any(t in f"{p.title} {p.body}".lower() for t in terms)
        ]
        return SearchResult(pages=hits[:limit], scanned=len(scope))

    def cite_knowledge(
        self, token: str, issue_id: str, page_id: str, note: str | None = None
    ) -> tuple[ReuseEvent, Issue]:
        agent = self._authed_agent(token)
        issue = self.store.get_issue(issue_id)
        if issue is None:
            raise errors.NotFound(f"issue '{issue_id}' not found")
        if issue.space_id not in agent.spaces:
            raise errors.Forbidden("issue belongs to a space you are not a member of")
        page = self.store.get_page(page_id)
        if page is None:
            raise errors.NotFound(f"page '{page_id}' not found")
        if not self._visible(page, agent):
            raise errors.Forbidden("page is not visible to you")

        event = ReuseEvent(
            id=self.store.new_id("reuse"),
            issue_id=issue.id,
            page_id=page.id,
            agent_id=agent.id,
            cross_team=page.space_id != issue.space_id,
        )
        self.store.add_reuse_event(event)
        issue.status = "knowledge_linked"
        self.store.save_issue(issue)
        return event, issue

    # --- 목록 · 조회 ---

    def list_spaces(self) -> list[Space]:
        return self.store.all_spaces()

    def get_space(self, space_id: str) -> Space:
        space = self.store.get_space(space_id)
        if space is None:
            raise errors.NotFound(f"space '{space_id}' not found")
        return space

    def list_issues(
        self,
        token: str,
        space_id: str | None = None,
        status: str | None = None,
        mine: bool = False,
    ) -> list[Issue]:
        agent = self._authed_agent(token)
        if space_id is not None and space_id not in agent.spaces:
            raise errors.Forbidden(f"not a member of space '{space_id}'")
        issues = [i for i in self.store.all_issues() if i.space_id in agent.spaces]
        if space_id is not None:
            issues = [i for i in issues if i.space_id == space_id]
        if status is not None:
            issues = [i for i in issues if i.status == status]
        if mine:
            issues = [i for i in issues if i.opened_by == agent.id]
        return issues

    def get_issue(self, token: str, issue_id: str) -> Issue:
        agent = self._authed_agent(token)
        issue = self.store.get_issue(issue_id)
        if issue is None:
            raise errors.NotFound(f"issue '{issue_id}' not found")
        if issue.space_id not in agent.spaces:
            raise errors.Forbidden("issue belongs to a space you are not a member of")
        return issue

    # --- 페이지 저작 · 트리 ---

    def create_page(
        self,
        token: str,
        space_id: str,
        title: str,
        body: str = "",
        parent_id: str | None = None,
        visibility: str = "org",
    ) -> Page:
        agent = self._authed_agent(token)
        if space_id not in agent.spaces:
            raise errors.Forbidden(f"not a member of space '{space_id}'")
        if parent_id is not None:
            parent = self.store.get_page(parent_id)
            if parent is None:
                raise errors.NotFound(f"parent page '{parent_id}' not found")
            if parent.space_id != space_id:
                raise errors.InvalidRequest("parent must be in the same space")
        page = Page(
            id=self.store.new_id("page"),
            space_id=space_id,
            title=title,
            body=body,
            source="authored",
            parent_id=parent_id,
            visibility=visibility,
        )
        self.store.add_page(page)
        return page

    def get_page(self, token: str, page_id: str) -> Page:
        agent = self._authed_agent(token)
        page = self.store.get_page(page_id)
        if page is None:
            raise errors.NotFound(f"page '{page_id}' not found")
        if not self._visible(page, agent):
            raise errors.Forbidden("page is not visible to you")
        return page

    def move_page(self, token: str, page_id: str, new_parent_id: str | None) -> Page:
        agent = self._authed_agent(token)
        page = self.store.get_page(page_id)
        if page is None:
            raise errors.NotFound(f"page '{page_id}' not found")
        if page.space_id not in agent.spaces:
            raise errors.Forbidden("page belongs to a space you are not a member of")
        if new_parent_id is not None:
            if new_parent_id == page_id:
                raise errors.InvalidRequest("a page cannot be its own parent")
            parent = self.store.get_page(new_parent_id)
            if parent is None:
                raise errors.NotFound(f"parent page '{new_parent_id}' not found")
            if parent.space_id != page.space_id:
                raise errors.InvalidRequest("parent must be in the same space")
            # 사이클 방지: 새 부모가 이 페이지의 자손이면 안 된다
            cur: Page | None = parent
            while cur is not None:
                if cur.id == page_id:
                    raise errors.InvalidRequest("move would create a cycle")
                cur = self.store.get_page(cur.parent_id) if cur.parent_id else None
        page.parent_id = new_parent_id
        self.store.save_page(page)
        return page

    def list_pages(self, token: str, space_id: str) -> list[Page]:
        agent = self._authed_agent(token)
        return [p for p in self.store.pages_in_space(space_id) if self._visible(p, agent)]

    # --- 멤버십 · 공간 관리 ---

    def update_space(self, space_id: str, name: str) -> Space:
        space = self.store.get_space(space_id)
        if space is None:
            raise errors.NotFound(f"space '{space_id}' not found")
        space.name = name
        self.store.add_space(space)
        return space

    def archive_space(self, space_id: str) -> Space:
        space = self.store.get_space(space_id)
        if space is None:
            raise errors.NotFound(f"space '{space_id}' not found")
        space.status = "archived"
        self.store.add_space(space)
        return space

    def add_member(self, token: str, space_id: str, agent_id: str) -> Agent:
        self._require_member(token, space_id)
        target = self.store.get_agent(agent_id)
        if target is None:
            raise errors.NotFound(f"agent '{agent_id}' not found")
        if space_id not in target.spaces:
            target.spaces.append(space_id)
            self.store.save_agent(target)
        return target

    def remove_member(self, token: str, space_id: str, agent_id: str) -> None:
        self._require_member(token, space_id)
        target = self.store.get_agent(agent_id)
        if target is None:
            raise errors.NotFound(f"agent '{agent_id}' not found")
        if space_id in target.spaces:
            target.spaces.remove(space_id)
            self.store.save_agent(target)

    def list_members(self, token: str, space_id: str) -> list[Agent]:
        self._require_member(token, space_id)
        return [a for a in self.store.all_agents() if space_id in a.spaces]

    # --- 내부 ---

    @staticmethod
    def _visible(page: Page, agent: Agent) -> bool:
        return page.visibility == "org" or page.space_id in agent.spaces

    def _authed_agent(self, token: str) -> Agent:
        agent = self.store.agent_for_token(token)
        if agent is None:
            raise errors.Unauthorized("invalid or missing token")
        return agent

    def _require_member(self, token: str, space_id: str) -> Agent:
        agent = self._authed_agent(token)
        if space_id not in agent.spaces:
            raise errors.Forbidden(f"not a member of space '{space_id}'")
        return agent
