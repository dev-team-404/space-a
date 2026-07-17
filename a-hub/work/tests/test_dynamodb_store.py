"""DynamoDB 영속 Store 어댑터 — moto로 검증 (boto3/moto 없으면 skip)."""

import os

import pytest

pytest.importorskip("boto3")
pytest.importorskip("moto")

os.environ.setdefault("AWS_DEFAULT_REGION", "us-east-1")
os.environ.setdefault("AWS_ACCESS_KEY_ID", "testing")
os.environ.setdefault("AWS_SECRET_ACCESS_KEY", "testing")

from moto import mock_aws  # noqa: E402

from ahub.adapters.store_dynamodb import DynamoDBStore, create_table  # noqa: E402
from ahub.core import errors  # noqa: E402
from ahub.core.services import SpaceAService  # noqa: E402

_REGION = "us-east-1"


@mock_aws
def test_full_flow_and_persistence_on_dynamodb():
    create_table("space-a", region=_REGION)
    svc = SpaceAService(DynamoDBStore("space-a", region=_REGION))
    svc.create_space("sw", "S/W", guidelines="규칙")
    _, tok = svc.register_agent("bot", "sw")
    iss = svc.open_issue(tok, "DS 인증서 오류", "sw")
    _, page = svc.resolve_issue(tok, iss.id, "인증서 갱신", ["재발급"])

    found = svc.search_knowledge(tok, "인증서")
    assert page.id in [p.id for p in found.pages]

    # 새 스토어 인스턴스(같은 테이블)도 본다 = 영속
    svc2 = SpaceAService(DynamoDBStore("space-a", region=_REGION))
    assert svc2.get_space("sw").name == "S/W"
    assert svc2.get_guide("sw") is not None
    assert svc2.get_issue(tok, iss.id).status == "resolved"


@mock_aws
def test_permission_scoping_on_dynamodb():
    create_table("t", region=_REGION)
    svc = SpaceAService(DynamoDBStore("t", region=_REGION))
    svc.create_space("a", "A")
    svc.create_space("b", "B")
    _, ta = svc.register_agent("x", "a")
    with pytest.raises(errors.Forbidden):
        svc.open_issue(ta, "침입", "b")


@mock_aws
def test_ids_and_page_tree_on_dynamodb():
    create_table("t", region=_REGION)
    svc = SpaceAService(DynamoDBStore("t", region=_REGION))
    svc.create_space("s", "S")
    _, tok = svc.register_agent("bot", "s")
    parent = svc.create_page(tok, "s", "부모")
    child = svc.create_page(tok, "s", "자식", parent_id=parent.id)
    assert parent.id.startswith("page_") and child.parent_id == parent.id
    ids = [p.id for p in svc.list_pages(tok, "s")]
    assert parent.id in ids and child.id in ids


@mock_aws
def test_rotate_token_revokes_old_on_dynamodb():
    create_table("t", region=_REGION)
    svc = SpaceAService(DynamoDBStore("t", region=_REGION))
    svc.create_space("s", "S")
    agent, tok = svc.register_agent("bot", "s")

    new = svc.rotate_token(tok, agent.id)  # revoke_tokens(batch) + bind_token

    assert new != tok
    with pytest.raises(errors.Unauthorized):
        svc.list_agents(tok)  # 옛 토큰 무효
    assert any(a.id == agent.id for a in svc.list_agents(new))
