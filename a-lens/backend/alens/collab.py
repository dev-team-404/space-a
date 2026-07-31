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
TOP_TERMS = 3           # 항목(문서·이슈) 하나에서 화면이 금색으로 짚을 말의 수
TOP_PAIRS = 3           # 엣지 하나당 근거로 보여줄 문서쌍 수 ("무엇이 통했나")
POOL_PAIRS = 24         # 그중에서 고르기 위해 들고 있는 후보 수 (§4.3 다양성 선택)

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
    하지 거쳐 최근 기존 상세 무사히 그러나 통한 위한 만큼 정도 이제 아직
    방식 적용 개선 원인 파악 반영 목록 해결 작업물 정상적 관련해 대응 조치 검토 논의 요청""".split()
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


def _diverse_pairs(pool: list[tuple[float, dict]]) -> list[dict]:
    """근거로 보여줄 문서쌍 고르기 — 점수순으로 뽑되 **같은 문서를 두 번 쓰지 않는다**.

    점수만으로 상위 3개를 뽑으면 인기 문서 하나가 세 쌍에 모두 끼어 같은 제목이 세 번 나온다
    (실측: 한 사람의 문서 하나가 3쌍을 다 차지했다). 그러면 근거 3개가 사실상 1개다.
    다양한 쌍이 모자라면 남은 것을 점수순으로 채운다 — 빈칸으로 두지는 않는다."""
    ordered = sorted(pool, key=lambda r: -r[0])
    out: list[dict] = []
    used: set[str] = set()
    for _, p in ordered:
        if len(out) >= TOP_PAIRS:
            break
        if used.intersection(p["docs"]):
            continue
        used.update(p["docs"])
        out.append(p)
    for _, p in ordered:
        if len(out) >= TOP_PAIRS:
            break
        if p not in out:
            out.append(p)
    return out


def build_idf(docs: list[dict]) -> tuple[dict[str, set[str]], dict[str, float]]:
    """방의 주제어 표 — 문서별 토큰 집합과 idf 가중치.

    협업 지도의 엣지 판정과 화면 전체의 주제어 강조가 **같은 표**를 쓰게 하려고 한곳에서
    만든다. 기준이 갈리면 지도에서 근거로 나온 말이 목록에서는 안 짚어진다."""
    toks = {d["doc_id"]: tokens(_doc_text(d)) for d in docs}
    df: collections.Counter = collections.Counter()
    for s in toks.values():
        df.update(s)
    n = len(docs)
    if n < 2:
        return toks, {}
    cut = max(DF_MIN, int(n * DF_MAX_RATIO))
    # idf는 ln((N+1)/df)로 완만하게 — 문서가 2건인 방에서 ln(N/df)는 0이 되어 어떤 겹침도
    # 점수를 못 받는다. N이 크면 두 식의 차이는 무시할 수준이다.
    return toks, {t: math.log((n + 1) / c) for t, c in df.items() if DF_MIN <= c <= cut}


def term_index(
    docs: list[dict], issues: list[dict], toks: dict[str, set[str]], idf: dict[str, float]
) -> dict[str, list[str]]:
    """항목(문서·이슈) → 그 항목을 다른 항목과 구별해주는 말 몇 개.

    화면 네 탭(작업 기록·이슈 공유·지식 재사용·문서함)이 이 색인으로 제목·요약의 같은 말을
    금색으로 짚는다. 흔한 말은 idf 컷에서 이미 빠졌으니 남은 것 중 상위만 고르면 된다."""
    out: dict[str, list[str]] = {}
    for d in docs:
        picked = sorted((t for t in toks.get(d["doc_id"], ()) if t in idf), key=lambda t: -idf[t])
        if picked:
            out[d["doc_id"]] = picked[:TOP_TERMS]
    for it in issues:
        if not it.get("issue_id"):
            continue
        # 이슈는 본문이 없다 — 제목만 같은 표로 훑는다
        picked = sorted((t for t in tokens(it.get("title", "")) if t in idf), key=lambda t: -idf[t])
        if picked:
            out[it["issue_id"]] = picked[:TOP_TERMS]
    return out


def _topic_edges(
    docs: list[dict],
    author_of: dict[str, str],
    toks: dict[str, set[str]],
    idf: dict[str, float],
    floor: float | None = None,
) -> tuple[list[dict], dict]:
    """주제 겹침(추정). 저자가 다른 문서쌍이 주제어를 충분히 공유하면 두 사람을 잇는다.

    반환 엣지는 무방향 — 정렬한 (a, b) 쌍을 키로 쓴다. weight는 자격을 얻은 문서쌍 수다.

    `floor`를 넘기면 그 문턱을 그대로 쓴다 — 프로젝트별 부분 지도가 방 전체와 **같은 잣대**를
    쓰게 하기 위해서다. 부분집합에서 다시 계산하면 문턱이 낮아져, 방 지도엔 없는 선이
    프로젝트 지도에만 생긴다(같은 데이터인데 화면마다 다른 결론)."""
    n = len(docs)
    if n < 2 or not idf:
        return [], {"keywords": 0}
    kept = {d["doc_id"]: {t for t in toks.get(d["doc_id"], ()) if t in idf} for d in docs}
    if floor is None:
        floor = MIN_SCORE_RATIO * math.log(n + 1)

    pairs: collections.Counter = collections.Counter()
    kw: dict[tuple[str, str], collections.Counter] = collections.defaultdict(collections.Counter)
    # 근거로 보여줄 문서쌍 — "무엇이 통했나"에 답하는 재료다. 점수 상위 몇 개만 들고 있는다.
    sample: dict[tuple[str, str], list[tuple[float, list[str]]]] = collections.defaultdict(list)
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
        # 점수 높은 쌍이 "왜 이어졌나"를 가장 잘 설명한다. 상위 몇 개만 남기고 버린다.
        # 쌍마다 **그 쌍이 공유한 말**을 같이 든다 — 선 전체 집계 키워드로는 개별 쌍이 왜
        # 묶였는지 설명이 안 된다(실데이터에서 제목만 보면 무관해 보이는 쌍이 나온다).
        ranked = sample[key]
        ranked.append(
            (
                score,
                {
                    "docs": [d1, d2] if a1 == key[0] else [d2, d1],  # 항상 (source쪽, target쪽)
                    "keywords": sorted(shared, key=lambda t: -idf[t])[:TOP_KEYWORDS],
                },
            )
        )
        if len(ranked) > POOL_PAIRS:
            ranked.sort(key=lambda r: -r[0])
            del ranked[POOL_PAIRS:]

    edges = [
        {
            "type": "topic",
            "source": a,
            "target": b,
            "weight": w,
            "keywords": [t for t, _ in kw[(a, b)].most_common(TOP_KEYWORDS)],
            # 근거 문서쌍 — {docs: [a쪽, b쪽], keywords: 그 쌍이 공유한 말}
            "doc_pairs": _diverse_pairs(sample[(a, b)]),
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


def _rank_edges(edges: list[dict]) -> tuple[list[dict], int]:
    """사실 먼저, 그 안에서 굵은 것 먼저. 상한을 넘으면 자르고 **자른 수를 돌려준다**."""
    ordered = sorted(edges, key=lambda e: (e["type"] == "topic", -e["weight"]))
    return ordered[:MAX_EDGES], max(0, len(ordered) - MAX_EDGES)


def _issues_by(issues: list[dict], canon: dict[str, str]) -> collections.Counter:
    return collections.Counter(
        w
        for it in issues
        if (w := _resolve(canon, it.get("opened_by"), (it.get("timeline") or [{}])[0].get("actor")))
    )


def _people_nodes(
    agents: list[dict],
    docs_by: collections.Counter,
    issues_by: collections.Counter,
    edges: list[dict],
    externals: dict[str, dict],
) -> list[dict]:
    """지도에 세울 사람들.

    남기는 기준은 **지도에 그릴 재료가 있는가**다: 문서를 썼거나(주제 겹침의 재료) 실제로
    선에 걸린 사람. 엣지가 없어도 문서가 있으면 남긴다 — 혼자 일한 것은 지울 상태가 아니라
    읽어야 할 정보다 (스펙 §7). 이슈만 몇 건 열고 문서가 없는 계정(실데이터의 `bot`·`reader`
    같은 테스트 계정)은 원을 채우기만 하고 어떤 선도 만들지 못하므로 뺀다."""
    used = {e["source"] for e in edges} | {e["target"] for e in edges}
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
        for a in agents
        if docs_by.get(a["agent_id"]) or a["agent_id"] in used
    ]
    nodes.sort(key=lambda n: n["name"])
    # 방 밖 사람은 엣지에 실제로 등장한 경우만, 바깥 고리에 뒤이어 붙인다
    nodes += [
        {**x, "status": "idle", "docs": 0, "issues": 0}
        for k, x in sorted(externals.items())
        if k in used
    ]
    return nodes


def _projects(docs: list[dict], author_of: dict[str, str]) -> dict:
    """프로젝트 축 — **a-mate 마커가 붙은 문서만** 센다.

    주제어로 프로젝트를 추정하지 않는다: 실데이터 157건으로 시험해보니 `이벤트`·`구현`처럼
    회고 요약체의 상용어가 프로젝트 행세를 했다(계획 §0). 마커는 세션 작업 디렉터리에서 온
    사실이므로, 사실만 프로젝트로 세고 나머지는 **모른다고 수만 보고한다**(스펙 §7 정직성).

    마커는 2026-07-31 이후 발행분에만 붙는다 — 그 이전 문서가 전부 `unknown_docs`로 잡히는
    것은 정상이다(소급 적용 없음)."""
    acc: dict[str, collections.Counter] = collections.defaultdict(collections.Counter)
    docs_of: collections.Counter = collections.Counter()
    unknown = 0
    for d in docs:
        proj = (d.get("project") or "").strip()
        if not proj:
            unknown += 1
            continue
        docs_of[proj] += 1
        who = author_of.get(d["doc_id"])
        if who:
            acc[proj][who] += 1
    nodes = [
        {
            "id": proj,
            "docs": n,
            # 같은 프로젝트에 문서를 남긴 사람들 — 사람×프로젝트 이분 그래프의 재료.
            "people": [{"id": w, "docs": c} for w, c in acc[proj].most_common()],
        }
        for proj, n in docs_of.most_common()
    ]
    return {"nodes": nodes, "unknown_docs": unknown}


def _project_graph(
    proj: str,
    knowledge: list[dict],
    issues: list[dict],
    reuse_events: list[dict],
    agents: list[dict],
    canon: dict[str, str],
    author_of: dict[str, str],
    toks: dict[str, set[str]],
    idf: dict[str, float],
    floor: float,
) -> dict:
    """프로젝트 하나만의 지도 — 태그를 누르면 이걸로 갈아 끼운다.

    방 지도와 **같은 주제어 표·같은 문턱**을 쓴다. 부분집합에서 다시 재면 잣대가 헐거워져
    방 지도엔 없는 선이 여기서만 생긴다 — 그러면 같은 데이터가 화면마다 다른 말을 한다.
    그래서 이 지도는 언제나 방 지도의 부분집합이다."""
    docs = [d for d in knowledge if (d.get("project") or "") == proj and d["doc_id"] in author_of]
    doc_ids = {d["doc_id"] for d in docs}
    iss = [it for it in issues if (it.get("project") or "") == proj]
    externals: dict[str, dict] = {}
    topic, _ = _topic_edges(docs, author_of, toks, idf, floor)
    reuse = _reuse_edges(
        [r for r in reuse_events if r.get("doc_id") in doc_ids], canon, author_of, externals
    )
    handoff, self_resolved = _handoff_edges(iss, canon)
    edges, dropped = _rank_edges(reuse + handoff + topic)
    docs_by = collections.Counter(author_of[d["doc_id"]] for d in docs)
    nodes = _people_nodes(agents, docs_by, _issues_by(iss, canon), edges, externals)
    return {
        "nodes": nodes,
        "edges": edges,
        "stats": {
            "reuse": sum(1 for e in edges if e["type"] == "reuse"),
            "handoff": sum(1 for e in edges if e["type"] == "handoff"),
            "topic": sum(1 for e in edges if e["type"] == "topic"),
            "issues": len(iss),
            "self_resolved": self_resolved,
            "docs": len(docs),
            "unlinked_docs": 0,  # 저자를 못 이은 문서는 프로젝트 지도에 들어오지 않는다
            "quiet_people": 0,
            "truncated_docs": 0,
            "dropped_edges": dropped,
            "keywords": 0,
        },
    }


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

    # 주제어 표는 **저자를 못 이은 문서까지 포함해** 만든다 — 강조는 사람과 무관하고,
    # 말뭉치가 넓을수록 흔한 말/드문 말 판정이 정확하다. 엣지는 저자가 있는 문서만 쓴다.
    corpus = linked + [d for d in knowledge if d["doc_id"] not in author_of][: max(0, MAX_DOCS - len(linked))]
    toks, idf = build_idf(corpus)
    terms = term_index(corpus, issues, toks, idf)

    externals: dict[str, dict] = {}
    # 방 전체의 문턱 — 프로젝트별 부분 지도도 이 잣대를 그대로 쓴다.
    floor = MIN_SCORE_RATIO * math.log(len(linked) + 1)
    topic, topic_stats = _topic_edges(linked, author_of, toks, idf, floor)
    reuse = _reuse_edges(reuse_events, canon, author_of, externals)
    handoff, self_resolved = _handoff_edges(issues, canon)

    edges, dropped = _rank_edges(reuse + handoff + topic)
    docs_by: collections.Counter = collections.Counter(author_of.values())
    issues_by = _issues_by(issues, canon)
    nodes = _people_nodes(agents, docs_by, issues_by, edges, externals)

    # 프로젝트 태그 + 태그마다의 부분 지도. 태그의 문서 수는 방의 **모든** 문서를 센다
    # (저자를 못 이은 것 포함 — 문서 축이므로). 지도는 그릴 수 있는 것만 그린다: 방 지도가
    # `docs`와 `unlinked_docs`를 따로 알리는 것과 같은 규율이다.
    projects = _projects(knowledge, author_of)
    for p in projects["nodes"]:
        p["graph"] = _project_graph(
            p["id"], linked, issues, reuse_events, agents, canon, author_of, toks, idf, floor
        )

    graph = {
        "nodes": nodes,
        "edges": edges,
        # 프로젝트 축 — 사람 축과 별개다. 마커 있는 문서만 들어간다(추정 아님).
        "projects": projects,
        # 항목별 주제어 — 사이드바 네 탭이 제목·요약에서 이 말들을 금색으로 짚는다
        "terms": terms,
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
