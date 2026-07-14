"""포트 — core가 의존하는 추상 인터페이스. 구현은 adapters에."""

from abc import ABC, abstractmethod

from .models import Agent, Issue, KnowledgeDoc, Space


class Store(ABC):
    # id / token 발급
    @abstractmethod
    def new_id(self, prefix: str) -> str: ...
    @abstractmethod
    def new_token(self) -> str: ...

    # spaces
    @abstractmethod
    def add_space(self, space: Space) -> None: ...
    @abstractmethod
    def get_space(self, space_id: str) -> Space | None: ...

    # agents / tokens
    @abstractmethod
    def add_agent(self, agent: Agent) -> None: ...
    @abstractmethod
    def bind_token(self, token: str, agent_id: str) -> None: ...
    @abstractmethod
    def agent_for_token(self, token: str) -> Agent | None: ...

    # issues
    @abstractmethod
    def add_issue(self, issue: Issue) -> None: ...
    @abstractmethod
    def get_issue(self, issue_id: str) -> Issue | None: ...
    @abstractmethod
    def save_issue(self, issue: Issue) -> None: ...

    # docs
    @abstractmethod
    def add_doc(self, doc: KnowledgeDoc) -> None: ...
