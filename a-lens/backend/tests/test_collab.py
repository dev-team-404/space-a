"""협업 지도 — 엣지 3종 산출과 정직성 규칙.

스펙: docs/design/a-lens/specs/2026-07-31-collab-graph.md

특히 지키려는 것: **추정을 사실로 승격하지 않는다.** 사실 엣지(reuse·handoff)는 서버에
기록이 있을 때만 생기고, 모호한 조인은 선을 긋지 않는다. 0건은 감추지 않고 stats로 보고한다.
"""

from alens import collab
from alens.collector import _issue_resolvers


# ── 토큰화 (스펙 §4.2) ───────────────────────────────────────


def test_predicates_are_dropped():
    """LLM 요약체의 서술어가 주제어를 먹지 않는다 — 실데이터에서 상위 키워드를 다 차지했다."""
    got = collab.tokens("작업을 진행했습니다. 성공적으로 마무리되었으나 오류가 발생했다")
    assert not {"진행했습니다", "마무리되었으나", "발생했다", "성공적"} & got


def test_two_letter_nouns_survive_tail_strip():
    """어미 1자 제거가 2자 명사를 죽이지 않는다 — `권한`·`제한`이 사라지면 안 된다."""
    got = collab.tokens("권한 제한 기한")
    assert {"권한", "제한", "기한"} <= got


def test_tail_strip_normalizes_inflections():
    assert "브랜치" in collab.tokens("브랜치를")
    assert collab.tokens("다양한") == set()  # 다양 → 불용어


def test_tail_strip_does_not_eat_nouns_ending_in_in():
    """`파이프라인`이 `파이프라`가 되면 안 된다 — 형용사형 `~적인`은 서술어 규칙이 잡는다."""
    assert "파이프라인" in collab.tokens("파이프라인 구성")
    assert "정상적인" not in collab.tokens("정상적인 동작")


def test_tool_run_counts_are_not_topics():
    assert not {"5회", "14회의"} & collab.tokens("도구를 5회 실행, 14회의 재시도")


def test_technical_terms_survive():
    got = collab.tokens("a-mate 로컬에서 build.rs 링크, playwright 실행")
    assert {"a-mate", "build.rs", "playwright"} <= got


def test_josa_on_english_term_is_stripped():
    """`powershell과`·`docker를`이 별개 주제어가 되면 같은 용어의 겹침이 흩어진다."""
    assert collab.tokens("powershell과 docker를 붙였다") >= {"powershell", "docker"}


# ── 이슈 ↔ 파생 문서 조인 (스펙 §3) ──────────────────────────


def test_resolver_joined_by_timestamp():
    issues = [{"issue_id": "i1", "updated_at": "2026-07-31T01:00:00Z"}]
    pages = [{"page_id": "p1", "created_at": "2026-07-31T01:00:00Z", "created_by": "bob"}]
    assert _issue_resolvers(issues, pages) == {"i1": {"agent_id": "bob", "page_id": "p1"}}


def test_ambiguous_timestamps_are_not_joined():
    """같은 시각에 이슈가 둘이면 잇지 않는다 — 추측으로 남의 해결을 붙이지 않는다."""
    ts = "2026-07-31T01:00:00Z"
    issues = [{"issue_id": "i1", "updated_at": ts}, {"issue_id": "i2", "updated_at": ts}]
    pages = [{"page_id": "p1", "created_at": ts, "created_by": "bob"}]
    assert _issue_resolvers(issues, pages) == {}


def test_two_pages_on_one_issue_are_not_joined():
    ts = "2026-07-31T01:00:00Z"
    issues = [{"issue_id": "i1", "updated_at": ts}]
    pages = [
        {"page_id": "p1", "created_at": ts, "created_by": "bob"},
        {"page_id": "p2", "created_at": ts, "created_by": "carol"},
    ]
    assert _issue_resolvers(issues, pages) == {}


# ── 그래프 조립 ──────────────────────────────────────────────


def _detail(**over) -> dict:
    base = {
        "space_id": "s1",
        "agents": [
            {"agent_id": "alice", "name": "앨리스", "status": "working"},
            {"agent_id": "bob", "name": "밥", "status": "idle"},
        ],
        "issues": [],
        "knowledge": [],
        "reuse_events": [],
    }
    base.update(over)
    collab._cache.clear()  # 지문 캐시가 테스트 간에 새지 않게
    return base


def test_handoff_edge_when_other_person_resolves():
    detail = _detail(
        issues=[
            {"issue_id": "i1", "status": "resolved", "opened_by": "alice", "resolved_by": "bob"},
            {"issue_id": "i2", "status": "resolved", "opened_by": "bob", "resolved_by": "bob"},
        ]
    )
    g = collab.build(detail)
    handoff = [e for e in g["edges"] if e["type"] == "handoff"]
    assert len(handoff) == 1
    assert (handoff[0]["source"], handoff[0]["target"]) == ("alice", "bob")
    assert g["stats"]["self_resolved"] == 1  # 자문자답은 셀프 루프가 아니라 카운트로 (§7)


def test_no_handoff_without_resolver():
    """`resolved_by`가 없으면(조인 모호·구버전) 아무 선도 긋지 않는다."""
    detail = _detail(issues=[{"issue_id": "i1", "status": "resolved", "opened_by": "alice"}])
    g = collab.build(detail)
    assert [e for e in g["edges"] if e["type"] == "handoff"] == []
    assert g["stats"]["handoff"] == 0


def test_reuse_edge_points_from_taker_to_writer():
    detail = _detail(
        knowledge=[{"doc_id": "d1", "title": "인증서 갱신", "author_agent_id": "alice"}],
        reuse_events=[{"doc_id": "d1", "by": "밥"}],
    )
    g = collab.build(detail)
    reuse = [e for e in g["edges"] if e["type"] == "reuse"]
    assert (reuse[0]["source"], reuse[0]["target"]) == ("bob", "alice")


def test_outside_reuser_becomes_external_node():
    """다른 방 사람이 우리 지식을 가져간 것이 이 엣지의 값어치다 — 지우지 않고 바깥에 세운다."""
    detail = _detail(
        knowledge=[{"doc_id": "d1", "title": "인증서 갱신", "author_agent_id": "alice"}],
        reuse_events=[{"doc_id": "d1", "consumer_agent": "윤성호"}],
    )
    g = collab.build(detail)
    ext = [n for n in g["nodes"] if n["external"]]
    assert [n["name"] for n in ext] == ["윤성호"]


def test_self_reuse_is_not_an_edge():
    detail = _detail(
        knowledge=[{"doc_id": "d1", "title": "t", "author_agent_id": "alice"}],
        reuse_events=[{"doc_id": "d1", "by": "앨리스"}],
    )
    assert collab.build(detail)["stats"]["reuse"] == 0


def test_topic_edge_between_different_authors():
    shared = "playwright 브라우저 자동화 스크린샷 파이프라인"
    detail = _detail(
        knowledge=[
            {"doc_id": "d1", "title": f"{shared} 도입", "author_agent_id": "alice"},
            {"doc_id": "d2", "title": f"{shared} 개선", "author_agent_id": "bob"},
            {"doc_id": "d3", "title": "회계 마감 절차 안내문", "author_agent_id": "bob"},
        ]
    )
    g = collab.build(detail)
    topic = [e for e in g["edges"] if e["type"] == "topic"]
    assert len(topic) == 1
    assert {topic[0]["source"], topic[0]["target"]} == {"alice", "bob"}
    assert topic[0]["keywords"]  # 근거(공유 주제어)가 비면 안 된다


def test_object_body_is_read_not_crashed():
    """C2 계약의 `body`는 문자열이 아니라 객체다(픽스처가 그 모양) — 둘 다 읽어야 한다."""
    shared = {"배경": "playwright 브라우저 자동화", "해결": "스크린샷 파이프라인 구성"}
    detail = _detail(
        knowledge=[
            {"doc_id": "d1", "title": "도입", "body": shared, "author_agent_id": "alice"},
            {"doc_id": "d2", "title": "개선", "body": shared, "author_agent_id": "bob"},
        ]
    )
    assert collab.build(detail)["stats"]["topic"] == 1


def test_same_author_docs_never_link():
    shared = "playwright 브라우저 자동화 스크린샷 파이프라인"
    detail = _detail(
        knowledge=[
            {"doc_id": "d1", "title": f"{shared} 도입", "author_agent_id": "alice"},
            {"doc_id": "d2", "title": f"{shared} 개선", "author_agent_id": "alice"},
        ]
    )
    assert collab.build(detail)["stats"]["topic"] == 0


def test_unlinked_authors_are_reported_not_guessed():
    """사람으로 못 이은 문서는 노드를 만들지 않고 수만 보고한다 (§5)."""
    detail = _detail(
        knowledge=[
            {"doc_id": "d1", "title": "t", "author_agent": "정체불명"},
            {"doc_id": "d2", "title": "t", "author_agent_id": "alice"},
        ]
    )
    g = collab.build(detail)
    assert g["stats"]["unlinked_docs"] == 1
    assert [n["id"] for n in g["nodes"]] == ["alice"]  # '정체불명' 노드를 만들지 않는다


def test_isolated_worker_stays_on_the_map():
    """엣지가 없어도 **일한 사람**은 남긴다 — 혼자 일한 것은 지울 상태가 아니라 정보다 (§7)."""
    detail = _detail(
        knowledge=[
            {"doc_id": "d1", "title": "회계 마감 절차 안내문", "author_agent_id": "alice"},
            {"doc_id": "d2", "title": "사내 셔틀버스 노선 변경", "author_agent_id": "bob"},
        ]
    )
    g = collab.build(detail)
    assert sorted(n["id"] for n in g["nodes"]) == ["alice", "bob"]
    assert g["edges"] == []


def test_people_with_no_activity_are_dropped_and_counted():
    """이 방에서 아무것도 안 한 계정(테스트·봇)은 원을 채우기만 한다 — 빼되 몇 명인지 알린다."""
    detail = _detail(knowledge=[{"doc_id": "d1", "title": "t", "author_agent_id": "alice"}])
    g = collab.build(detail)
    assert [n["id"] for n in g["nodes"]] == ["alice"]
    assert g["stats"]["quiet_people"] == 1  # 밥은 기록이 없다


def test_issue_only_account_without_edges_is_dropped():
    """실데이터의 `bot`처럼 이슈만 몇 건 열고 문서가 없는 계정 — 어떤 선도 만들지 못한다."""
    detail = _detail(
        knowledge=[{"doc_id": "d1", "title": "t", "author_agent_id": "alice"}],
        issues=[{"issue_id": "i1", "status": "resolved", "opened_by": "bob", "resolved_by": "bob"}],
    )
    g = collab.build(detail)
    assert [n["id"] for n in g["nodes"]] == ["alice"]


def test_issue_only_account_stays_when_it_has_an_edge():
    """같은 계정이라도 실제로 선에 걸리면(남이 해결해줬다) 남는다."""
    detail = _detail(
        knowledge=[{"doc_id": "d1", "title": "t", "author_agent_id": "alice"}],
        issues=[{"issue_id": "i1", "status": "resolved", "opened_by": "bob", "resolved_by": "alice"}],
    )
    g = collab.build(detail)
    assert sorted(n["id"] for n in g["nodes"]) == ["alice", "bob"]


def test_topic_edge_carries_document_pairs_as_evidence():
    """"무엇이 통했나"에 답할 재료 — 근거 문서쌍이 (source쪽, target쪽) 순서로 온다."""
    shared = "playwright 브라우저 자동화 스크린샷 파이프라인"
    detail = _detail(
        knowledge=[
            {"doc_id": "d-alice", "title": f"{shared} 도입", "author_agent_id": "alice"},
            {"doc_id": "d-bob", "title": f"{shared} 개선", "author_agent_id": "bob"},
        ]
    )
    e = [x for x in collab.build(detail)["edges"] if x["type"] == "topic"][0]
    first = e["doc_pairs"][0]
    assert first["docs"] == (["d-alice", "d-bob"] if e["source"] == "alice" else ["d-bob", "d-alice"])
    # 쌍마다 그 쌍이 공유한 말이 따로 붙는다 — 선 전체 집계로는 개별 쌍을 설명 못 한다
    assert "playwright" in first["keywords"]


def test_merged_accounts_are_one_node():
    detail = _detail(
        agents=[
            {"agent_id": "alice", "name": "앨리스", "merged_ids": ["a-mate/alice-old"], "status": "idle"},
            {"agent_id": "bob", "name": "밥", "status": "idle"},
        ],
        knowledge=[
            {"doc_id": "d1", "title": "playwright 자동화 스크린샷 파이프라인", "author_agent_id": "alice-old"},
            {"doc_id": "d2", "title": "playwright 자동화 스크린샷 파이프라인 개선", "author_agent_id": "bob"},
        ],
    )
    g = collab.build(detail)
    topic = [e for e in g["edges"] if e["type"] == "topic"]
    assert {topic[0]["source"], topic[0]["target"]} == {"alice", "bob"}  # 옛 계정도 같은 노드
    assert g["stats"]["unlinked_docs"] == 0


def test_facts_are_ranked_before_guesses():
    detail = _detail(
        issues=[{"issue_id": "i1", "status": "resolved", "opened_by": "alice", "resolved_by": "bob"}],
        knowledge=[
            {"doc_id": "d1", "title": "playwright 자동화 스크린샷 파이프라인", "author_agent_id": "alice"},
            {"doc_id": "d2", "title": "playwright 자동화 스크린샷 파이프라인 개선", "author_agent_id": "bob"},
        ],
    )
    edges = collab.build(detail)["edges"]
    assert edges[0]["type"] == "handoff"  # 사실이 추정보다 앞


def test_cache_recomputes_when_resolver_appears():
    """자문자답이던 이슈를 남이 해결한 것으로 바꾸면 캐시가 아니라 새 그래프가 나와야 한다."""
    detail = _detail(issues=[{"issue_id": "i1", "status": "resolved", "opened_by": "alice", "resolved_by": "alice"}])
    assert collab.build(detail)["stats"]["handoff"] == 0
    detail2 = dict(detail, issues=[{"issue_id": "i1", "status": "resolved", "opened_by": "alice", "resolved_by": "bob"}])
    assert collab.build(detail2)["stats"]["handoff"] == 1


def test_term_index_marks_distinctive_words():
    """네 탭이 같은 기준으로 금색 강조를 하려면 항목별 주제어가 그래프와 같은 표에서 나와야 한다."""
    detail = _detail(
        knowledge=[
            {"doc_id": "d1", "title": "playwright 스크린샷 파이프라인 도입", "author_agent_id": "alice"},
            {"doc_id": "d2", "title": "playwright 스크린샷 파이프라인 개선", "author_agent_id": "bob"},
            {"doc_id": "d3", "title": "회계 마감 절차 안내문", "author_agent_id": "bob"},
        ],
        issues=[{"issue_id": "i1", "title": "playwright 실행 실패", "status": "open", "opened_by": "alice"}],
    )
    terms = collab.build(detail)["terms"]
    assert "playwright" in terms["d1"]
    assert "playwright" in terms["i1"]  # 이슈는 본문이 없어 제목만 훑는다
    assert "d3" not in terms or "playwright" not in terms["d3"]


def test_term_index_covers_docs_without_a_known_author():
    """강조는 사람과 무관하다 — 저자를 못 이은 문서도 주제어를 받는다."""
    detail = _detail(
        knowledge=[
            {"doc_id": "d1", "title": "playwright 스크린샷 파이프라인", "author_agent": "정체불명"},
            {"doc_id": "d2", "title": "playwright 스크린샷 도입", "author_agent_id": "alice"},
        ]
    )
    g = collab.build(detail)
    assert g["stats"]["unlinked_docs"] == 1
    assert g["terms"].get("d1")


# ── 프로젝트 축 (계획: docs/archive/design/common/plans/2026-07-31-project-axis-from-cwd.md) ──


def test_project_axis_counts_only_marked_docs():
    """마커 있는 문서만 프로젝트로 센다. 나머지는 추정으로 채우지 않고 수만 보고한다."""
    detail = _detail(
        knowledge=[
            {"doc_id": "d1", "title": "포트 충돌", "project": "space-a", "author_agent_id": "alice"},
            {"doc_id": "d2", "title": "배포 정리", "project": "space-a", "author_agent_id": "bob"},
            {"doc_id": "d3", "title": "미터 대시보드", "project": "agent-meter", "author_agent_id": "bob"},
            {"doc_id": "d4", "title": "마커 없는 옛 문서", "author_agent_id": "alice"},
        ]
    )
    projects = collab.build(detail)["projects"]
    assert [(n["id"], n["docs"]) for n in projects["nodes"]] == [("space-a", 2), ("agent-meter", 1)]
    assert projects["unknown_docs"] == 1  # 소급 적용이 없으므로 '모른다'가 정답이다

    space_a = projects["nodes"][0]
    assert sorted(p["id"] for p in space_a["people"]) == ["alice", "bob"]


def test_project_axis_is_empty_before_any_marker_arrives():
    """지금 실데이터가 그렇다 — 프로젝트 0건에 미분류 전부. 빈 축이 화면을 깨뜨리면 안 된다."""
    detail = _detail(
        knowledge=[{"doc_id": "d1", "title": "옛 문서", "author_agent_id": "alice"}]
    )
    projects = collab.build(detail)["projects"]
    assert projects["nodes"] == []
    assert projects["unknown_docs"] == 1


def test_project_marker_is_parsed_and_removed_from_the_title():
    """마커는 기계용이다 — 프로젝트로 읽어내고 제목에서는 지운다."""
    from alens.collector import _parse_project

    # 페이지 제목(= 회고 요약)
    assert _parse_project("[a-mate:proj=space-a] 포트 충돌을 고쳤다") == (
        "space-a",
        "포트 충돌을 고쳤다",
    )
    # 이슈 제목 — 회고 접두와 붙어 나온다
    assert _parse_project("[a-mate:proj=space-a][a-mate 회고] 포트 충돌") == (
        "space-a",
        "[a-mate 회고] 포트 충돌",
    )
    # 마커 없는 옛 문서는 제목 그대로, 프로젝트는 '모른다'
    assert _parse_project("[a-mate 회고] 옛 문서") == (None, "[a-mate 회고] 옛 문서")
    assert _parse_project("") == (None, "")
    # 다른 a-mate 마커(R8 공유)를 프로젝트로 오독하지 않는다
    assert _parse_project("[a-mate:R8:github] 대형 결과") == (None, "[a-mate:R8:github] 대형 결과")


def test_project_graph_is_a_subset_of_the_room_graph():
    """태그를 눌러 좁힌 지도에 방 지도엔 없던 선이 생기면 안 된다 — 같은 잣대를 써야 한다."""
    detail = _detail(
        knowledge=[
            {"doc_id": "d1", "title": "playwright 스크린샷 파이프라인 도입", "project": "space-a",
             "author_agent_id": "alice"},
            {"doc_id": "d2", "title": "playwright 스크린샷 파이프라인 개선", "project": "space-a",
             "author_agent_id": "bob"},
            {"doc_id": "d3", "title": "회계 마감 절차 안내문", "project": "agent-meter",
             "author_agent_id": "bob"},
        ]
    )
    g = collab.build(detail)
    room = {(e["type"], e["source"], e["target"]) for e in g["edges"]}
    by_id = {p["id"]: p for p in g["projects"]["nodes"]}

    space_a = by_id["space-a"]["graph"]
    assert {(e["type"], e["source"], e["target"]) for e in space_a["edges"]} <= room
    assert space_a["stats"]["docs"] == 2
    assert sorted(n["id"] for n in space_a["nodes"]) == ["alice", "bob"]

    # 문서가 하나뿐인 프로젝트는 선이 생길 수 없다 — 그래도 그 사람은 지도에 남는다(§7).
    meter = by_id["agent-meter"]["graph"]
    assert meter["edges"] == []
    assert [n["id"] for n in meter["nodes"]] == ["bob"]
