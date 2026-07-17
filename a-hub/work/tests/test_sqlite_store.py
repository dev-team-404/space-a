"""영속 저장소 어댑터 (SqliteStore) — 재접속 후에도 데이터가 남는다."""

from ahub.adapters.store_sqlite import SqliteStore
from ahub.core.services import SpaceAService


def test_full_flow_on_sqlite():
    svc = SpaceAService(SqliteStore(":memory:"))
    svc.create_space("sw-innov", "S/W")
    _, tok = svc.register_agent("bot", "bot", "sw-innov")
    issue = svc.open_issue(tok, "DS 인증서 오류", "sw-innov")
    _, page = svc.resolve_issue(tok, issue.id, "인증서 갱신")

    found = svc.search_knowledge(tok, "인증서")
    assert page.id in [p.id for p in found.pages]


def test_persists_across_reconnect(tmp_path):
    db = str(tmp_path / "ahub.db")

    s1 = SpaceAService(SqliteStore(db))
    s1.create_space("sw-innov", "S/W", purpose="p", guidelines="g")
    _, tok = s1.register_agent("bot", "bot", "sw-innov")
    issue = s1.open_issue(tok, "문제", "sw-innov")
    _, page = s1.resolve_issue(tok, issue.id, "영속 해결책")
    child = s1.create_page(tok, "sw-innov", "자식")

    # 완전히 새 연결로 다시 연다
    s2 = SpaceAService(SqliteStore(db))
    # 같은 토큰이 남아 인증된다
    found = s2.search_knowledge(tok, "영속")
    assert page.id in [p.id for p in found.pages]
    # 공간·가이드·페이지 모두 살아있다
    assert s2.get_space("sw-innov").name == "S/W"
    assert s2.get_guide("sw-innov") is not None
    assert child.id in [p.id for p in s2.list_pages(tok, "sw-innov")]


def test_ids_are_unique_and_prefixed():
    svc = SpaceAService(SqliteStore(":memory:"))
    svc.create_space("s", "S")
    a1, _ = svc.register_agent("a1", "a1", "s")
    a2, _ = svc.register_agent("a2", "a2", "s")
    assert a1.id == "a1" and a2.id == "a2"
    assert a1.id != a2.id


def test_concurrent_read_write_is_safe(tmp_path):
    """공유 연결을 여러 스레드가 읽기·쓰기해도 오류 없고 데이터가 온전하다."""
    import threading

    svc = SpaceAService(SqliteStore(str(tmp_path / "c.db")))
    svc.create_space("s", "S")
    _, tok = svc.register_agent("bot", "bot", "s")
    errors: list[Exception] = []

    def worker(n: int) -> None:
        try:
            for i in range(20):
                iss = svc.open_issue(tok, f"t{n}-{i}", "s")
                svc.resolve_issue(tok, iss.id, f"sol{n}-{i}")
                svc.search_knowledge(tok, "sol")  # 동시 읽기
                svc.list_issues(tok)
        except Exception as e:  # noqa: BLE001
            errors.append(e)

    threads = [threading.Thread(target=worker, args=(n,)) for n in range(8)]
    for t in threads:
        t.start()
    for t in threads:
        t.join()

    assert not errors
    assert len(svc.list_issues(tok)) == 8 * 20  # 유실 없이 전부 기록
