"""협업 지도 — 사람 노드 + 엣지 3종(인용·핸드오프·주제 겹침).

스펙: docs/design/a-lens/specs/2026-07-31-collab-graph.md — 이 문서가 정답이다.

**사실과 추정을 엣지 종류로 가른다.** `reuse`·`handoff`는 서버에 기록된 사건이고 `topic`은
문서 텍스트에서 유도한 추정이다. 섞으면 "없는 협업이 있어 보이는" 화면이 되므로, 합치는
판단은 여기서도 프론트에서도 하지 않는다.

허브 실데이터의 사실 엣지는 지금 0건이다 (이슈 145건 전부 자문자답 · `/reuse-events` 404).
데이터를 못 읽어서가 아니라 **협업이 실제로 없었던 것**이므로 0을 감추지 않고 stats로 보고한다.
LLM은 쓰지 않는다 — 토큰화·IDF·임계값 전부 결정론이다.
"""

from __future__ import annotations

import collections
import itertools
import math
import re

from .life_client import normalize as _norm  # 사람 조인 규칙은 공통 신원과 같은 것을 쓴다

# ── 튜닝 상수 (실데이터 156건 탐색 결과 — 스펙 §4.3) ─────────
MAX_DOCS = 400          # 방당 비교 상한. O(D²)라 넘치면 최신순으로 자르고 stats에 보고한다
DF_MIN = 2              # 1회만 나온 말은 우연이다
DF_MAX_RATIO = 0.30     # 30% 넘는 문서에 나오면 주제어가 아니라 말버릇이다
MIN_SHARED = 2          # 문서쌍이 공유해야 할 최소 주제어 수
# 공유 주제어 idf 합의 하한 — **문서 수로 정규화한 값**이다. idf는 ln(N)에 비례해 커지므로
# 절대값으로 자르면 문서가 156건인 방에서 맞춘 임계가 20건짜리 방에서는 절대 안 걸린다.
# 실데이터(N=156) 튜닝값 5.0 ÷ ln(157) ≈ 1.0 — 큰 방의 동작은 그대로 두고 작은 방만 살린다.
MIN_SCORE_RATIO = 1.0
MAX_EDGES = 12          # 화면에 그릴 엣지 수 — 넘으면 weight 상위만 (잘린 수는 stats에)
TOP_KEYWORDS = 5

# ── 한국어 근사 토큰화 (스펙 §4.2) ───────────────────────────
# 형태소 분석기를 안 쓴다: 새 의존성(JVM·사전)을 이 기능 하나로 들이지 않는다. 두 문서가
# 같은 주제인지만 판정하면 되므로 접미 규칙 + 문서빈도 컷으로 충분하다.

_JOSA = (
    "에서는", "으로는", "에게서", "까지", "부터", "에게", "한테", "으로", "에서", "라는",
    "이라는", "들의", "들이", "들을", "에는", "와의", "과의", "의", "를", "을", "이", "가",
    "은", "는", "에", "로", "와", "과", "도", "만", "께", "들",
)
# 어미 1자만 뗀다. 2자 토큰은 건드리지 않는다 — `권한`·`제한`이 죽는다.
# `인`은 넣지 않는다: `적인`은 아래 서술어 규칙이 이미 잡고, 여기 넣으면 `파이프라인`이
# `파이프라`가 된다(더미 데이터에서 실제로 그랬다).
_TAIL = ("한", "하")
# 서술어로 판정되면 버린다. a-mate 세션 요약체가 이 형태로 어휘를 지배한다.
_PRED_END = re.compile(r"(습니다|했|였|겠|하며|하여|하고|되어|된|는|다|요|적|적인|스러|처럼|같이)$")
_PRED_IN = re.compile(r"(했|였|었|겠|습니|드립|됐)")
_COUNT = re.compile(r"^\d+[가-힣]+$")  # '5회'·'14회의' — 도구 실행 횟수는 주제가 아니다
_WORD = re.compile(r"[^0-9a-z가-힣\-\._/]+")
_HANGUL = re.compile(r"^[가-힣]+$")
# 영문 용어에 조사가 붙은 혼합 토큰(`powershell과`·`docker를`). 한영 혼용이 흔한 말뭉치라
# 이걸 안 떼면 같은 용어가 조사별로 다른 주제어가 되어 겹침이 흩어진다.
_MIXED = re.compile(r"^([0-9a-z][0-9a-z\-\._/]{2,})[가-힣]+$")

# 요약체 상용어. 활용형이 갈려 문서빈도 컷에 안 걸리는 것들만 손으로 막는다.
_STOP = set(
    """것 수 등 및 통해 대한 위해 대해 관련 확인 사용 발생 진행 처리 작업 문제 내용 경우 이번
    해당 그리고 하지만 또한 다시 모두 전체 각각 이후 이전 현재 상태 결과 방법 총 약 개 건 회 명
    번 중 시 분 초 오늘 어제 이날 동안 정상 완료 실패 성공 시작 종료 추가 수정 변경 사용자 유저
    세션 기록 요약 정리 이슈 문서 지식 여러 차례 다양 최종 회복 규모 어려움 마무리 시도 이상 이하
    부분 이유 과정 오류 도구 에러 실행 여부 필요 가능 지원 기능 대상 기준 관리 설정 같은 함께
    하지 거쳐 최근 기존 상세 무사히 그러나 통한 위한 만큼 정도 이제 아직""".split()
)


def tokens(text: str) -> set[str]:
    """주제어 후보 집합. 명사 근사 — 조사·어미를 떼고 서술어를 버린다."""
    out: set[str] = set()
    for raw in _WORD.sub(" ", (text or "").lower()).split():
        t = raw.strip("-._/")
        if not t or _COUNT.match(t):
            continue
        if t.replace(".", "").replace("-", "").isdigit():
            continue
        if _HANGUL.match(t):
            for j in _JOSA:
                if len(t) - len(j) >= 2 and t.endswith(j):
                    t = t[: -len(j)]
                    break
            for j in _TAIL:
                if len(t) >= 3 and t.endswith(j):
                    t = t[:-1]
                    break
            if not (2 <= len(t) <= 8) or t in _STOP:
                continue
            if _PRED_END.search(t) or _PRED_IN.search(t):
                continue
        else:
            mixed = _MIXED.match(t)
            if mixed:
                t = mixed.group(1)
            if len(t) < 3 or t in _STOP:
                continue
        out.add(t)
    return out


# ── 사람 동일시 (스펙 §5) ────────────────────────────────────


def _index(agents: list[dict]) -> dict[str, str]:
    """조인 키(정규화) → 대표 agent_id. `_merge_people`이 이미 합쳐놓은 결과를 그대로 따른다.

    같은 사람이 허브 계정을 여럿 갖는 일이 흔해서(등록 경로가 여럿), 노드가 갈라지면
    그래프가 통째로 거짓말이 된다. 여기서 **새 추측은 만들지 않는다** — 이름·흡수된 id로도
    못 이으면 노드를 만들지 않고 stats의 `unlinked_docs`로 보고한다."""
    canon: dict[str, str] = {}
    # 두 바퀴로 도는 게 핵심 — **계정 id가 남의 표시 이름을 항상 이긴다.** 한 바퀴로 돌면
    # 목록 앞쪽 사람의 이름이 뒷사람의 id 자리를 차지한다(실측: 허브에 `agt_9`(이름
    # 'kimmy-claude')와 `kimmy-claude`가 둘 다 있어 문서 36건이 엉뚱한 노드로 갔다).
    for a in agents:
        cid = a.get("agent_id")
        if cid:
            for key in (cid, *(a.get("merged_ids") or [])):
                if key:
                    canon.setdefault(_norm(str(key)), cid)
    for a in agents:
        cid = a.get("agent_id")
        if cid:
            for key in (a.get("hub_name"), a.get("name")):
                if key:
                    canon.setdefault(_norm(str(key)), cid)
    return canon


def _resolve(canon: dict[str, str], *keys: str | None) -> str | None:
    for k in keys:
        if k:
            hit = canon.get(_norm(str(k)))
            if hit:
                return hit
    return None


def _doc_seq(doc_id: str) -> int:
    m = re.search(r"(\d+)$", doc_id or "")
    return int(m.group(1)) if m else 0


# ── 엣지 ─────────────────────────────────────────────────────


def _doc_text(d: dict) -> str:
    """주제어를 뽑을 원문. `body`는 소스마다 모양이 다르다 — 허브·더미는 마크다운 문자열이지만
    **C2 계약은 객체**(`{섹션: 내용}`)로 규정한다(픽스처가 그 모양이다). 둘 다 받는다."""
    body = d.get("body")
    if isinstance(body, dict):
        body = " ".join(str(v) for v in body.values())
    elif not isinstance(body, str):
        body = ""
    return f"{d.get('title', '')} {d.get('summary', '')} {body[:4000]}"


def _topic_edges(docs: list[dict], author_of: dict[str, str]) -> tuple[list[dict], dict]:
    """주제 겹침(추정). 저자가 다른 문서쌍이 주제어를 충분히 공유하면 두 사람을 잇는다.

    반환 엣지는 무방향 — 정렬한 (a, b) 쌍을 키로 쓴다. weight는 자격을 얻은 문서쌍 수다."""
    toks = {d["doc_id"]: tokens(_doc_text(d)) for d in docs}
    df: collections.Counter = collections.Counter()
    for s in toks.values():
        df.update(s)
    n = len(docs)
    if n < 2:
        return [], {"keywords": 0}
    cut = max(DF_MIN, int(n * DF_MAX_RATIO))
    # idf는 ln((N+1)/df)로 완만하게 — 문서가 2건인 방에서 ln(N/df)는 0이 되어 어떤 겹침도
    # 점수를 못 받는다. N이 크면 두 식의 차이는 무시할 수준이다.
    idf = {t: math.log((n + 1) / c) for t, c in df.items() if DF_MIN <= c <= cut}
    kept = {doc_id: {t for t in s if t in idf} for doc_id, s in toks.items()}
    floor = MIN_SCORE_RATIO * math.log(n + 1)

    pairs: collections.Counter = collections.Counter()
    kw: dict[tuple[str, str], collections.Counter] = collections.defaultdict(collections.Counter)
    sample: dict[tuple[str, str], dict] = {}
    for (d1, s1), (d2, s2) in itertools.combinations(kept.items(), 2):
        a1, a2 = author_of[d1], author_of[d2]
        if a1 == a2:
            continue
        shared = s1 & s2
        if len(shared) < MIN_SHARED:
            continue
        score = sum(idf[t] for t in shared)
        if score < floor:
            continue
        key = (a1, a2) if a1 < a2 else (a2, a1)
        pairs[key] += 1
        kw[key].update(shared)
        # 대표 문서쌍은 점수가 가장 높은 것 — "왜 이어졌나"를 가장 잘 설명하는 쌍이다
        best = sample.get(key)
        if best is None or score > best["score"]:
            sample[key] = {"score": score, "doc_ids": [d1, d2]}

    edges = [
        {
            "type": "topic",
            "source": a,
            "target": b,
            "weight": w,
            "keywords": [t for t, _ in kw[(a, b)].most_common(TOP_KEYWORDS)],
            "doc_ids": sample[(a, b)]["doc_ids"],
        }
        for (a, b), w in pairs.items()
    ]
    return edges, {"keywords": len(idf)}


def _reuse_edges(
    reuse_events: list[dict],
    canon: dict[str, str],
    author_of_doc: dict[str, str],
    externals: dict[str, dict],
) -> list[dict]:
    """인용(사실). 지식을 **가져다 쓴 사람** → **쓴 사람**. 북극성 지표의 사람 단위 표현.

    가져간 사람이 다른 방 사람인 경우가 이 엣지의 진짜 값어치다("우리 지식이 밖에서 쓰였다").
    그래서 방 밖 사람은 지우지 않고 external 노드로 남긴다."""
    tally: collections.Counter = collections.Counter()
    docs: dict[tuple[str, str], list[str]] = collections.defaultdict(list)
    for ev in reuse_events:
        doc_id = ev.get("doc_id")
        writer = author_of_doc.get(doc_id or "")
        taker_name = ev.get("by") or ev.get("consumer_agent") or ev.get("cited_by_name")
        if not writer or not taker_name:
            continue
        taker = _resolve(canon, taker_name)
        if taker is None:  # 방 밖 사람 — 이름만 아는 채로 바깥 고리에 세운다
            taker = f"ext:{_norm(str(taker_name))}"
            externals.setdefault(taker, {"id": taker, "name": str(taker_name), "external": True})
        if taker == writer:
            continue  # 자기 지식 재인용은 협업이 아니다
        tally[(taker, writer)] += 1
        if doc_id:
            docs[(taker, writer)].append(doc_id)
    return [
        {"type": "reuse", "source": s, "target": t, "weight": w, "doc_ids": docs[(s, t)][:3]}
        for (s, t), w in tally.items()
    ]


def _handoff_edges(issues: list[dict], canon: dict[str, str]) -> tuple[list[dict], int]:
    """이슈 핸드오프(사실). 연 사람 → 해결한 사람. 같은 사람이면 엣지가 아니라 자문자답 카운트.

    `resolved_by`는 collector가 타임스탬프 조인으로 채운다 (스펙 §3). 모호하면 비어 있고,
    비어 있으면 여기서 아무 선도 긋지 않는다 — 추측으로 남의 해결을 붙이지 않는다."""
    tally: collections.Counter = collections.Counter()
    refs: dict[tuple[str, str], list[str]] = collections.defaultdict(list)
    self_resolved = 0
    for it in issues:
        opener = _resolve(canon, it.get("opened_by"), (it.get("timeline") or [{}])[0].get("actor"))
        resolver = _resolve(canon, it.get("resolved_by"))
        if not opener or not resolver:
            continue
        if opener == resolver:
            self_resolved += 1
            continue
        tally[(opener, resolver)] += 1
        refs[(opener, resolver)].append(it.get("issue_id", ""))
    edges = [
        {"type": "handoff", "source": s, "target": t, "weight": w, "issue_ids": refs[(s, t)][:3]}
        for (s, t), w in tally.items()
    ]
    return edges, self_resolved


# ── 조립 ─────────────────────────────────────────────────────

_cache: dict[str, tuple[str, dict]] = {}


def _fingerprint(space_id: str, docs: list[dict], issues: list[dict], reuse: list[dict]) -> str:
    """재계산이 필요한 변화만 잡는 지문. 스냅숏은 30초마다 갱신되지만 방 내용은 그대로일 때가
    대부분이라, 요청마다 O(D²)를 다시 돌지 않게 한다."""
    parts = [
        space_id,
        str(len(docs)),
        str(len(issues)),
        str(len(reuse)),
        docs[-1].get("doc_id", "") if docs else "",
        issues[-1].get("issue_id", "") if issues else "",
        "".join(sorted(str(i.get("resolved_by") or "") for i in issues))[:200],
    ]
    return "|".join(parts)


def build(detail: dict, snapshot: dict | None = None) -> dict:
    """방 하나의 협업 그래프. 실패해도 방을 못 열게 하지 않는다 — 빈 그래프로 강등한다."""
    space_id = detail.get("space_id", "")
    agents = [a for a in (detail.get("agents") or []) if a.get("agent_id")]
    issues = detail.get("issues") or []
    knowledge = [d for d in (detail.get("knowledge") or []) if d.get("doc_id")]
    reuse_events = detail.get("reuse_events")
    if reuse_events is None and snapshot:  # 픽스처 경로 — detail에 재사용이 안 실려 온다
        reuse_events = [
            r
            for r in (snapshot.get("reuse_events") or [])
            if space_id in (r.get("source_space"), r.get("consumer_space"))
        ]
    reuse_events = reuse_events or []

    # 캐시 슬롯은 방×tier — 게스트는 이슈가 비워져 오므로 같은 슬롯을 쓰면 서로를 밀어낸다
    slot = f"{space_id}:{detail.get('viewer_tier', '')}"
    key = _fingerprint(space_id, knowledge, issues, reuse_events)
    hit = _cache.get(slot)
    if hit and hit[0] == key:
        return hit[1]

    canon = _index(agents)
    # 문서 → 저자(대표 id). 못 이은 문서는 통계에서 빼고 수만 보고한다.
    author_of: dict[str, str] = {}
    unlinked = 0
    for d in knowledge:
        who = _resolve(canon, d.get("author_agent_id"), d.get("author_agent"))
        if who:
            author_of[d["doc_id"]] = who
        else:
            unlinked += 1

    # O(D²) 상한 — 넘치면 최신순으로 자르되 잘린 수를 화면에 알린다(조용한 절단 금지)
    linked = [d for d in knowledge if d["doc_id"] in author_of]
    truncated = 0
    if len(linked) > MAX_DOCS:
        linked.sort(key=lambda d: _doc_seq(d["doc_id"]), reverse=True)
        truncated = len(linked) - MAX_DOCS
        linked = linked[:MAX_DOCS]

    externals: dict[str, dict] = {}
    topic, topic_stats = _topic_edges(linked, author_of)
    reuse = _reuse_edges(reuse_events, canon, author_of, externals)
    handoff, self_resolved = _handoff_edges(issues, canon)

    edges = reuse + handoff + topic
    edges.sort(key=lambda e: (e["type"] == "topic", -e["weight"]))  # 사실 먼저, 그 안에서 굵은 것 먼저
    dropped = max(0, len(edges) - MAX_EDGES)
    edges = edges[:MAX_EDGES]

    docs_by: collections.Counter = collections.Counter(author_of.values())
    issues_by: collections.Counter = collections.Counter(
        w for it in issues if (w := _resolve(canon, it.get("opened_by"), (it.get("timeline") or [{}])[0].get("actor")))
    )
    used = {e["source"] for e in edges} | {e["target"] for e in edges}
    # 엣지가 없어도 **일한 사람은 남긴다** — 혼자 일한 것은 지울 상태가 아니라 읽어야 할
    # 정보다 (스펙 §7). 반대로 이 방에서 아무것도 안 한 계정(테스트·봇 등록 계정이 실데이터에
    # 6개 있다)은 원을 채우기만 하므로 빼고, 몇 명을 뺐는지 stats로 알린다.
    active = [a for a in agents if docs_by.get(a["agent_id"]) or issues_by.get(a["agent_id"]) or a["agent_id"] in used]
    nodes = [
        {
            "id": a["agent_id"],
            "name": a.get("name") or a["agent_id"],
            "status": a.get("status", "idle"),
            "mascot_url": a.get("mascot_url"),
            "docs": docs_by.get(a["agent_id"], 0),
            "issues": issues_by.get(a["agent_id"], 0),
            "external": False,
        }
        for a in active
    ]
    nodes.sort(key=lambda n: n["name"])
    # 방 밖 사람은 엣지에 실제로 등장한 경우만, 바깥 고리에 뒤이어 붙인다
    nodes += [{**x, "status": "idle", "docs": 0, "issues": 0} for k, x in sorted(externals.items()) if k in used]

    graph = {
        "nodes": nodes,
        "edges": edges,
        "stats": {
            "reuse": sum(1 for e in edges if e["type"] == "reuse"),
            "handoff": sum(1 for e in edges if e["type"] == "handoff"),
            "topic": sum(1 for e in edges if e["type"] == "topic"),
            "issues": len(issues),
            "self_resolved": self_resolved,
            "docs": len(knowledge),
            "unlinked_docs": unlinked,
            "quiet_people": len(agents) - len([n for n in nodes if not n["external"]]),
            "truncated_docs": truncated,
            "dropped_edges": dropped,
            "keywords": topic_stats.get("keywords", 0),
        },
    }
    _cache[slot] = (key, graph)
    return graph
