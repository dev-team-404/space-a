"""Space A 도메인 서비스.

관리(create_space, register_agent), 이슈 생애주기(open_issue, resolve_issue),
검색·재사용(search_knowledge, cite_knowledge).
권한 규칙: 소속(spaces)은 토큰에서 유도하며, 요청 인자로 주장할 수 없다.
"""

import re
from datetime import datetime, timezone
from typing import Callable

from . import errors
from .models import Agent, Issue, Page, ReuseEvent, SearchResult, SkillCandidate, Space
from .ports import Store

# user_id는 agent.id이자 URL 경로 파라미터(/agents/{id} 등)로 쓰이므로,
# 라우팅·키를 깨뜨리는 문자를 막고 길이를 제한한다.
_USER_ID_RE = re.compile(r"^[A-Za-z0-9_.-]{1,100}$")


def _utc_now() -> str:
    return datetime.now(timezone.utc).isoformat()


class SpaceAService:
    def __init__(self, store: Store, *, now: Callable[[], str] | None = None):
        self.store = store
        self._now = now or _utc_now  # 주입 가능한 clock (테스트 결정성)

    def _touch(self, obj: Issue | Page, ts: str | None = None) -> None:
        """상태 변경 저장 직전 updated_at을 갱신한다 (created_at은 보존).

        ts를 주면 그 값을 쓴다 — 한 트랜잭션에서 여러 엔티티가 같은 시각을
        공유해야 할 때(예: resolve_issue의 이슈+발행 페이지).
        """
        obj.updated_at = ts or self._now()

    # --- 관리 (control plane) ---

    def create_space(
        self, space_id: str, name: str, purpose: str = "", guidelines: str = ""
    ) -> Space:
        space = Space(id=space_id, name=name, purpose=purpose, guidelines=guidelines)
        self.store.add_space(space)
        if guidelines:
            _ts = self._now()
            guide = Page(
                id=self.store.new_id("page"),
                space_id=space_id,
                title="가이드",
                body=guidelines,
                source="authored",
                created_at=_ts,
                updated_at=_ts,
            )
            self.store.add_page(guide)
            space.guide_page_id = guide.id
            self.store.add_space(space)
        return space

    def get_guide(self, space_id: str) -> Page | None:
        space = self.store.get_space(space_id)
        if space is None or space.guide_page_id is None:
            return None
        return self.store.get_page(space.guide_page_id)

    def register_agent(
        self, user_id: str, name: str, space_id: str
    ) -> tuple[Agent, str]:
        """사용자 지정 user_id로 계정을 등록/재사용한다.

        user_id = agent.id (안정적 식별자). 같은 user_id로 다시 부르면 새 계정을
        만들지 않고 재사용(name 갱신·공간 병합)한다. 토큰은 매번 새로 발급하되
        기존 토큰도 유효하다 — 한 사람이 봇을 여러 개 돌려도 같은 계정으로 기록된다.
        """
        if not user_id or not user_id.strip():
            raise errors.InvalidRequest("user_id is required")
        if not _USER_ID_RE.match(user_id):
            raise errors.InvalidRequest(
                "user_id must be 1-100 chars of letters, digits, '_', '-', or '.'"
            )
        if self.store.get_space(space_id) is None:
            raise errors.NotFound(f"space '{space_id}' not found")
        agent = self.store.get_agent(user_id)
        if agent is None:
            agent = Agent(id=user_id, name=name, spaces=[space_id])
        else:
            agent.name = name
            if space_id not in agent.spaces:
                agent.spaces.append(space_id)
        self.store.save_agent(agent)  # upsert
        token = self.store.new_token()
        self.store.bind_token(token, agent.id)  # 새 토큰, 기존 토큰 유지
        return agent, token

    # --- 이슈 생애주기 ---

    def open_issue(self, token: str, title: str, space_id: str) -> Issue:
        agent = self._authed_agent(token)
        if space_id not in agent.spaces:
            raise errors.Forbidden(f"not a member of space '{space_id}'")
        _ts = self._now()
        issue = Issue(
            id=self.store.new_id("iss"),
            space_id=space_id,
            title=title,
            opened_by=agent.id,
            created_at=_ts,
            updated_at=_ts,
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

        _ts = self._now()
        issue.status = "resolved"
        self._touch(issue, _ts)
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
                created_by=agent.id,
                created_at=_ts,
                updated_at=_ts,
            )
            self.store.add_page(page)
        return issue, page

    # --- 검색 · 재사용 ---

    def search_knowledge(
        self, token: str, query: str, space_id: str | None = None, limit: int = 3
    ) -> SearchResult:
        agent = self._authed_agent(token)
        scope = [
            p
            for p in self.store.all_pages()
            if p.status == "active" and self._visible(p, agent)
        ]
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
        if page.status != "active":
            raise errors.InvalidRequest(f"cannot cite a non-active page (status: {page.status})")

        event = ReuseEvent(
            id=self.store.new_id("reuse"),
            issue_id=issue.id,
            page_id=page.id,
            agent_id=agent.id,
            cross_team=page.space_id != issue.space_id,
        )
        self.store.add_reuse_event(event)
        issue.status = "knowledge_linked"
        self._touch(issue)
        self.store.save_issue(issue)
        return event, issue

    def get_skill_candidates(
        self, token: str, space_id: str | None = None, min_occurrences: int = 3
    ) -> list[SkillCandidate]:
        agent = self._authed_agent(token)
        if space_id is not None and space_id not in agent.spaces:
            raise errors.Forbidden(f"not a member of space '{space_id}'")
        pages = [
            p
            for p in self.store.all_pages()
            if p.status == "active"
            and p.source == "issue-derived"
            and p.space_id in agent.spaces
        ]
        if space_id is not None:
            pages = [p for p in pages if p.space_id == space_id]
        groups: dict[str, list[str]] = {}
        for p in pages:
            groups.setdefault(p.title, []).append(p.id)
        return [
            SkillCandidate(pattern=title, occurrences=len(ids), page_ids=ids)
            for title, ids in groups.items()
            if len(ids) >= min_occurrences
        ]

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
            if parent.status != "active":
                raise errors.InvalidRequest("parent page is not active")
        _ts = self._now()
        page = Page(
            id=self.store.new_id("page"),
            space_id=space_id,
            title=title,
            body=body,
            source="authored",
            parent_id=parent_id,
            visibility=visibility,
            created_by=agent.id,
            created_at=_ts,
            updated_at=_ts,
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
            if parent.status != "active":
                raise errors.InvalidRequest("parent page is not active")
            # 사이클 방지: 새 부모가 이 페이지의 자손이면 안 된다.
            # visited로 이미 손상된 데이터의 무한루프(DoS)도 함께 차단한다.
            visited = {page_id}
            cur: Page | None = parent
            while cur is not None:
                if cur.id in visited:
                    raise errors.InvalidRequest("move would create a cycle")
                visited.add(cur.id)
                cur = self.store.get_page(cur.parent_id) if cur.parent_id else None
        page.parent_id = new_parent_id
        self._touch(page)
        self.store.save_page(page)
        return page

    def list_pages(self, token: str, space_id: str) -> list[Page]:
        agent = self._authed_agent(token)
        return [
            p
            for p in self.store.pages_in_space(space_id)
            if p.status == "active" and self._visible(p, agent)
        ]

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

    # --- 페이지 lifecycle ---

    def edit_page(
        self, token: str, page_id: str, title: str | None = None, body: str | None = None
    ) -> Page:
        _, page = self._page_for_member(token, page_id)
        if title is not None:
            page.title = title
        if body is not None:
            page.body = body
        self._touch(page)
        self.store.save_page(page)
        return page

    def set_visibility(self, token: str, page_id: str, visibility: str) -> Page:
        _, page = self._page_for_member(token, page_id)
        page.visibility = visibility
        self._touch(page)
        self.store.save_page(page)
        return page

    def archive_page(self, token: str, page_id: str) -> Page:
        _, page = self._page_for_member(token, page_id)
        page.status = "archived"
        self._touch(page)
        self.store.save_page(page)
        return page

    def supersede_page(self, token: str, page_id: str, by_page_id: str) -> Page:
        _, page = self._page_for_member(token, page_id)
        if by_page_id == page_id:
            raise errors.InvalidRequest("a page cannot supersede itself")
        by_page = self.store.get_page(by_page_id)
        if by_page is None:
            raise errors.NotFound(f"page '{by_page_id}' not found")
        if by_page.space_id != page.space_id:
            raise errors.InvalidRequest("superseding page must be in the same space")
        page.status = "superseded"
        page.superseded_by = by_page_id
        self._touch(page)
        self.store.save_page(page)
        return page

    def quarantine_page(self, token: str, page_id: str) -> Page:
        _, page = self._page_for_member(token, page_id)
        page.status = "quarantined"
        self._touch(page)
        self.store.save_page(page)
        return page

    def flag_page(self, token: str, page_id: str) -> Page:
        agent = self._authed_agent(token)
        page = self.store.get_page(page_id)
        if page is None:
            raise errors.NotFound(f"page '{page_id}' not found")
        if not self._visible(page, agent):
            raise errors.Forbidden("page is not visible to you")
        page.flags += 1
        self._touch(page)
        self.store.save_page(page)
        return page

    # --- 에이전트 lifecycle ---

    def list_agents(self, token: str) -> list[Agent]:
        agent = self._authed_agent(token)
        mine = set(agent.spaces)
        return [a for a in self.store.all_agents() if mine & set(a.spaces)]

    def get_agent(self, token: str, agent_id: str) -> Agent:
        self._authed_agent(token)
        target = self.store.get_agent(agent_id)
        if target is None:
            raise errors.NotFound(f"agent '{agent_id}' not found")
        return target

    def agent_name(self, agent_id: str | None) -> str | None:
        """저자 agent_id → 사람이 읽을 name. 없거나 사라진 계정이면 None.

        조회 응답에 created_by/opened_by(agent_id) 옆에 이름을 병기할 때 쓴다.
        신원(auth)이 아니라 표시용 조회라 토큰을 요구하지 않는다.
        """
        if not agent_id:
            return None
        agent = self.store.get_agent(agent_id)
        return agent.name if agent is not None else None

    def revoke_agent(self, token: str, agent_id: str) -> None:
        caller = self._authed_agent(token)
        if agent_id != caller.id:
            raise errors.Forbidden("you can only revoke your own agent")
        self.store.revoke_tokens(agent_id)

    def rotate_token(self, token: str, agent_id: str) -> str:
        caller = self._authed_agent(token)
        if agent_id != caller.id:
            raise errors.Forbidden("you can only rotate your own token")
        self.store.revoke_tokens(agent_id)
        new_token = self.store.new_token()
        self.store.bind_token(new_token, agent_id)
        return new_token

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

    def _page_for_member(self, token: str, page_id: str) -> tuple[Agent, Page]:
        agent = self._authed_agent(token)
        page = self.store.get_page(page_id)
        if page is None:
            raise errors.NotFound(f"page '{page_id}' not found")
        if page.space_id not in agent.spaces:
            raise errors.Forbidden("page belongs to a space you are not a member of")
        return agent, page
