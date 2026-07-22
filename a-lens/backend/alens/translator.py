"""분류 → 요약 → 서사 번역 — 로컬 LLM(OpenAI 호환) 1콜 + 규칙 폴백 + 증분 캐시.

a-hub raw Page 하나를 사람이 관전하기 좋은 형태로 옮긴다:
  ① category  문서 종류 (사람이 알아들을 폐쇄 카테고리)
  ② summary   2~3문장 요약 (body[:120] 대체)
  ③ narrative 한 줄 서사

증분: store에 같은 page_id + updated_at이 있으면 LLM을 부르지 않고 캐시를 쓴다
(a-hub에서 바뀐/새 문서만 LLM에 태운다). 설정(LLM URL·모델)은 settings에서 호출 시점에 읽는다.
설계: docs/design/a-lens/05-narrative-summarization.md
"""

import json
import logging
import re

import httpx

from . import settings

log = logging.getLogger("alens.translator")

# 공간(팀)과 직교하는 표시용 폐쇄 카테고리 (05 §3.1)
CATEGORIES = ["troubleshoot", "spec", "analysis", "release", "note", "guide"]
CATEGORY_KO = {
    "troubleshoot": "문제해결",
    "spec": "설계·스펙",
    "analysis": "분석·결과",
    "release": "릴리즈·변경",
    "note": "노트·회의",
    "guide": "가이드",
}

_SYSTEM = (
    "너는 사내 지식 문서를 사람이 관전하기 좋게 번역하는 도우미다. "
    "본문에 실제로 있는 사실·수치·고유명사만 쓰고, 없는 내용을 지어내지 마라. "
    "반드시 지정한 JSON 형식만 출력한다."
)


def _rule(title: str, body: str, kind: str) -> dict:
    """LLM 없거나 실패 시 규칙 폴백 — 분류는 종류·키워드, 요약은 앞 120자."""
    low = (title or "").lower()
    if kind == "guide":
        cat = "guide"
    elif kind in ("issue", "issue-derived"):
        cat = "troubleshoot"
    elif any(k in low for k in ("diff", "릴리즈", "release", "changelog", "노트 v")):
        cat = "release"
    elif any(k in low for k in ("스펙", "spec", "아키텍처", "architecture")):
        cat = "spec"
    elif any(k in low for k in ("분석", "qor", "결과", "report", "리포트")):
        cat = "analysis"
    else:
        cat = "note"
    narrative = f"‘{title}’ 내용을 정리해 팀에 공유했어요." if title else None
    return {"category": cat, "summary": (body or "")[:120], "narrative": narrative}


def _extract_json(s: str) -> dict:
    m = re.search(r"\{.*\}", s or "", re.S)
    if not m:
        return {}
    try:
        return json.loads(m.group(0))
    except Exception:  # noqa: BLE001
        return {}


def _llm(cfg: dict, title: str, body: str, kind: str) -> dict:
    url = cfg["llm_url"].rstrip("/") + "/chat/completions"
    headers = {"Content-Type": "application/json"}
    if cfg.get("llm_key"):
        headers["Authorization"] = f"Bearer {cfg['llm_key']}"
    user = (
        f"제목: {title}\n종류: {kind}\n본문:\n{(body or '')[:4000]}\n\n"
        "위 문서를 번역해 아래 JSON만 출력해라.\n"
        f'{{"category": <{"|".join(CATEGORIES)} 중 하나>, '
        '"summary": "팀 관전용 2~3문장 한국어 요약", '
        '"narrative": "누가 무엇을 했는지 한 줄 서사"}'
    )
    payload = {
        "model": cfg["llm_model"],
        "temperature": 0.2,
        "messages": [
            {"role": "system", "content": _SYSTEM},
            {"role": "user", "content": user},
        ],
    }
    r = httpx.post(url, json=payload, headers=headers, timeout=90)
    r.raise_for_status()
    content = r.json()["choices"][0]["message"]["content"]
    obj = _extract_json(content)
    cat = obj.get("category", "note")
    if cat not in CATEGORIES:
        cat = "note"
    return {
        "category": cat,
        "summary": (obj.get("summary") or (body or "")[:120]).strip(),
        "narrative": (obj.get("narrative") or None),
    }


def translate_text(title: str, body: str, kind: str = "authored") -> tuple[dict, str]:
    """{category, summary, narrative}와 사용 모델명을 반환한다 (store 미접촉 — 순수 변환).

    캐시·본문 재조회 판단(무엇을 LLM에 태울지)은 호출자 collector._page_doc이 store로 한다."""
    cfg = settings.get()
    if cfg.get("llm_url"):
        try:
            return _llm(cfg, title, body, kind), cfg["llm_model"]
        except Exception as e:  # noqa: BLE001 — LLM 실패는 규칙 폴백으로 흡수
            log.warning("LLM 번역 실패, 규칙 폴백: %s", e)
    return _rule(title, body, kind), "rule"
