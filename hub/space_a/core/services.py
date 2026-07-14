"""Space A 도메인 서비스.

관리(create_space, register_agent)와 지식 생애주기(open_issue, resolve_issue).
권한 규칙: 소속(spaces)은 토큰에서 유도하며, 요청 인자로 주장할 수 없다.
"""

from . import errors
from .models import Agent, Issue, KnowledgeDoc, Space
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

    # --- 지식 생애주기 ---

    def open_issue(self, token: str, title: str, space_id: str) -> Issue:
        agent = self._authed_agent(token)
        if space_id not in agent.spaces:
            raise errors.Forbidden(f"not a member of space '{space_id}'")
        issue = Issue(id=self.store.new_id("iss"), space_id=space_id, title=title)
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
    ) -> tuple[Issue, KnowledgeDoc | None]:
        agent = self._authed_agent(token)
        issue = self.store.get_issue(issue_id)
        if issue is None:
            raise errors.NotFound(f"issue '{issue_id}' not found")
        if issue.space_id not in agent.spaces:
            raise errors.Forbidden("issue belongs to a space you are not a member of")

        issue.status = "resolved"
        self.store.save_issue(issue)

        doc: KnowledgeDoc | None = None
        if publish_knowledge:
            doc = KnowledgeDoc(
                id=self.store.new_id("doc"),
                issue_id=issue.id,
                space_id=issue.space_id,
                summary=summary,
                steps=steps or [],
                visibility=visibility,
            )
            self.store.add_doc(doc)
        return issue, doc

    # --- 내부 ---

    def _authed_agent(self, token: str) -> Agent:
        agent = self.store.agent_for_token(token)
        if agent is None:
            raise errors.Unauthorized("invalid or missing token")
        return agent
