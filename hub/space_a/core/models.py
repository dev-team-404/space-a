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


@dataclass
class ReuseEvent:
    """이슈가 기존 문서를 인용한 순간. 이 시스템의 북극성 지표."""

    id: str
    issue_id: str
    doc_id: str
    agent_id: str
    cross_team: bool = False


@dataclass
class SearchResult:
    docs: list[KnowledgeDoc]
    scanned: int  # 권한 범위 안에서 훑은 문서 수 (토큰 절감 증명용 카운터)
