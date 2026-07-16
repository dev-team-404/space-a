"""DynamoDB 영속 Store 어댑터 (서버리스용).

단일 테이블 설계: `pk`=엔티티 타입, `sk`=id.
- get_X(id)  = GetItem(pk=TYPE, sk=id)
- all_X()    = Query(pk=TYPE)
- new_id     = UpdateItem ADD (원자적 카운터)
포트&어댑터라 core는 그대로. 테이블은 앱이 아니라 IaC(SAM)가 만든다
(테스트/로컬 편의를 위해 create_table 헬퍼 제공).
"""

import os
from dataclasses import asdict

import boto3
from boto3.dynamodb.conditions import Key

from ..core.models import Agent, Issue, Page, ReuseEvent, Space
from ..core.ports import Store

_SPACE, _AGENT, _TOKEN, _ISSUE, _PAGE, _REUSE, _SEQ = (
    "SPACE", "AGENT", "TOKEN", "ISSUE", "PAGE", "REUSE", "SEQ",
)


def _region(region: str | None) -> str:
    return region or os.environ.get("AWS_DEFAULT_REGION") or os.environ.get("AWS_REGION") or "us-east-1"


def create_table(name: str, region: str | None = None) -> None:
    """테스트/로컬용 테이블 생성. 프로덕션은 SAM이 만든다."""
    ddb = boto3.client("dynamodb", region_name=_region(region))
    ddb.create_table(
        TableName=name,
        KeySchema=[
            {"AttributeName": "pk", "KeyType": "HASH"},
            {"AttributeName": "sk", "KeyType": "RANGE"},
        ],
        AttributeDefinitions=[
            {"AttributeName": "pk", "AttributeType": "S"},
            {"AttributeName": "sk", "AttributeType": "S"},
        ],
        BillingMode="PAY_PER_REQUEST",
    )
    ddb.get_waiter("table_exists").wait(TableName=name)


class DynamoDBStore(Store):
    def __init__(self, table_name: str | None = None, region: str | None = None) -> None:
        table_name = table_name or os.environ["SPACE_A_TABLE"]
        self._t = boto3.resource("dynamodb", region_name=_region(region)).Table(table_name)

    # --- id / token ---

    def new_id(self, prefix: str) -> str:
        r = self._t.update_item(
            Key={"pk": _SEQ, "sk": prefix},
            UpdateExpression="ADD n :one",
            ExpressionAttributeValues={":one": 1},
            ReturnValues="UPDATED_NEW",
        )
        return f"{prefix}_{int(r['Attributes']['n'])}"

    def new_token(self) -> str:
        return self.new_id("tok")

    # --- generic ---

    def _put(self, type_: str, id_: str, obj) -> None:
        item = dict(asdict(obj))
        item["pk"] = type_
        item["sk"] = id_
        self._t.put_item(Item=item)

    def _get(self, type_: str, id_: str, cls):
        r = self._t.get_item(Key={"pk": type_, "sk": id_}).get("Item")
        return self._to(cls, r) if r else None

    def _all_items(self, type_: str) -> list[dict]:
        items: list[dict] = []
        kwargs = {"KeyConditionExpression": Key("pk").eq(type_)}
        while True:
            resp = self._t.query(**kwargs)
            items += resp.get("Items", [])
            lek = resp.get("LastEvaluatedKey")
            if not lek:
                break
            kwargs["ExclusiveStartKey"] = lek
        return items

    def _all(self, type_: str, cls):
        return [self._to(cls, it) for it in self._all_items(type_)]

    @staticmethod
    def _to(cls, item: dict):
        d = {k: v for k, v in item.items() if k not in ("pk", "sk")}
        if d.get("flags") is not None:
            d["flags"] = int(d["flags"])  # Decimal → int
        return cls(**d)

    # --- spaces ---

    def add_space(self, space: Space) -> None:
        self._put(_SPACE, space.id, space)

    def get_space(self, space_id: str) -> Space | None:
        return self._get(_SPACE, space_id, Space)

    def all_spaces(self) -> list[Space]:
        return self._all(_SPACE, Space)

    # --- agents / tokens ---

    def add_agent(self, agent: Agent) -> None:
        self._put(_AGENT, agent.id, agent)

    def bind_token(self, token: str, agent_id: str) -> None:
        self._t.put_item(Item={"pk": _TOKEN, "sk": token, "agent_id": agent_id})

    def agent_for_token(self, token: str) -> Agent | None:
        r = self._t.get_item(Key={"pk": _TOKEN, "sk": token}).get("Item")
        return self.get_agent(r["agent_id"]) if r else None

    def get_agent(self, agent_id: str) -> Agent | None:
        return self._get(_AGENT, agent_id, Agent)

    def save_agent(self, agent: Agent) -> None:
        self._put(_AGENT, agent.id, agent)

    def all_agents(self) -> list[Agent]:
        return self._all(_AGENT, Agent)

    def revoke_tokens(self, agent_id: str) -> None:
        for it in self._all_items(_TOKEN):
            if it.get("agent_id") == agent_id:
                self._t.delete_item(Key={"pk": _TOKEN, "sk": it["sk"]})

    # --- issues ---

    def add_issue(self, issue: Issue) -> None:
        self._put(_ISSUE, issue.id, issue)

    def get_issue(self, issue_id: str) -> Issue | None:
        return self._get(_ISSUE, issue_id, Issue)

    def save_issue(self, issue: Issue) -> None:
        self._put(_ISSUE, issue.id, issue)

    def all_issues(self) -> list[Issue]:
        return self._all(_ISSUE, Issue)

    # --- pages ---

    def add_page(self, page: Page) -> None:
        self._put(_PAGE, page.id, page)

    def get_page(self, page_id: str) -> Page | None:
        return self._get(_PAGE, page_id, Page)

    def save_page(self, page: Page) -> None:
        self._put(_PAGE, page.id, page)

    def all_pages(self) -> list[Page]:
        return self._all(_PAGE, Page)

    def pages_in_space(self, space_id: str) -> list[Page]:
        return [p for p in self.all_pages() if p.space_id == space_id]

    # --- reuse events ---

    def add_reuse_event(self, event: ReuseEvent) -> None:
        self._put(_REUSE, event.id, event)
