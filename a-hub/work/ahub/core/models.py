"""도메인 모델. 순수 데이터 홀더 — 로직은 services에."""

from dataclasses import dataclass, field


@dataclass
class Space:
    id: str
    name: str
    status: str = "active"
    purpose: str = ""              # 이 방이 무엇을 위한 곳인가
    guidelines: str = ""           # 무엇을·어떻게 쓸지 (에이전트용 지침)
    guide_page_id: str | None = None  # guidelines로 seed된 가이드 Page


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
    opened_by: str | None = None  # 이슈를 연 agent_id


@dataclass
class Page:
    """공간 안의 문서. 문서는 전부 Page다 (별도 Knowledge 개념 없음).

    - source=authored     : 사람/에이전트가 직접 저작 → 트리에 배치
    - source=issue-derived : 이슈 해결의 산물(resolve_issue)
    """

    id: str
    space_id: str
    title: str
    body: str = ""
    source: str = "authored"          # authored | issue-derived
    parent_id: str | None = None      # 트리 배치 (None = 최상위)
    status: str = "active"            # active | archived
    visibility: str = "org"           # org | space
    issue_id: str | None = None       # issue-derived일 때 원본 이슈
    steps: list[str] = field(default_factory=list)  # 해결 단계 (issue-derived)
    superseded_by: str | None = None  # 대체된 경우 새 문서 id
    flags: int = 0                    # 오답 신고 누적
    created_by: str | None = None     # 작성한 agent_id (issue-derived면 resolve한 agent)


@dataclass
class ReuseEvent:
    """이슈가 기존 Page를 인용한 순간. 이 시스템의 북극성 지표."""

    id: str
    issue_id: str
    page_id: str
    agent_id: str
    cross_team: bool = False


@dataclass
class SearchResult:
    pages: list[Page]
    scanned: int  # 권한 범위 안에서 훑은 문서 수 (토큰 절감 증명용 카운터)


@dataclass
class SkillCandidate:
    """반복 기록된 해결 패턴. N회 이상이면 Skill로 승격 후보."""

    pattern: str
    occurrences: int
    page_ids: list[str]
