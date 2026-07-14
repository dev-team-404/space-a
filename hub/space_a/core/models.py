"""도메인 모델. 순수 데이터 홀더 — 로직은 services에."""

from dataclasses import dataclass, field


@dataclass
class Space:
    id: str
    name: str
    status: str = "active"


@dataclass
class Agent:
    id: str
    name: str
    spaces: list[str] = field(default_factory=list)


@dataclass
class Issue:
    id: str
    space_id: str
    title: str
    status: str = "open"


@dataclass
class KnowledgeDoc:
    id: str
    issue_id: str
    space_id: str
    summary: str
    steps: list[str] = field(default_factory=list)
    visibility: str = "org"
