#!/usr/bin/env python3
"""다이어리 재생성용 diary_index 삭제 도구 (Windows).

앱을 종료한 상태에서 실행하세요. `findings`와 vault의 `.md` 파일은 건드리지 않습니다
(diary_index 행만 삭제 → 앱 재시작 시 startup backfill이 룩백 창 안의 날짜를 재생성).

주의: 기본 backfill 룩백은 7일이라, 그보다 오래된 날짜를 삭제해도 자동 재생성되지 않고
      달력에서 사라지기만 합니다(--all 이어도 최근 7일만 다시 채워짐). 전체 재생성이
      필요하면 백엔드 룩백 값을 함께 늘려야 합니다.

사용법:
  py scripts/reset-diary.py            # 최근 7일(오늘 -7 ~ -1) 삭제 (기본)
  py scripts/reset-diary.py --days 30  # 최근 30일 삭제
  py scripts/reset-diary.py --all      # 전체 삭제
"""
import argparse
import os
import sqlite3
import subprocess
import sys
from datetime import date, timedelta

DB = os.path.join(os.environ.get("APPDATA", ""), "dev.agentmentor.app", "agent-mentor.db")


def app_running() -> bool:
    """agent-mentor 앱이 떠 있으면 True (backfill은 시작 시에만 돌아 켜진 채 삭제하면 무의미)."""
    try:
        out = subprocess.run(["tasklist"], capture_output=True, text=True).stdout.lower()
        return "agent-mentor" in out
    except Exception:
        return False  # tasklist 실패 시 막지 않음(경고만)


def main() -> None:
    ap = argparse.ArgumentParser(description="diary_index 삭제(다이어리 재생성 준비)")
    g = ap.add_mutually_exclusive_group()
    g.add_argument("--all", action="store_true", help="전체 삭제")
    g.add_argument("--days", type=int, default=7, help="최근 N일 삭제 (기본 7)")
    args = ap.parse_args()

    if not os.path.exists(DB):
        print(f"DB 없음: {DB}", file=sys.stderr)
        sys.exit(1)

    if app_running():
        print("앱(agent-mentor)이 실행 중입니다. 종료 후 다시 실행하세요.", file=sys.stderr)
        print("(켜진 채 삭제하면 backfill이 재생성하지 않습니다.)", file=sys.stderr)
        sys.exit(2)

    c = sqlite3.connect(DB)
    f_before = c.execute("SELECT COUNT(*) FROM findings").fetchone()[0]

    if args.all:
        n = c.execute("DELETE FROM diary_index").rowcount
        scope = "전체"
    else:
        today = date.today()
        lo = (today - timedelta(days=args.days)).isoformat()
        hi = (today - timedelta(days=1)).isoformat()
        n = c.execute(
            "DELETE FROM diary_index WHERE date BETWEEN ? AND ?", (lo, hi)
        ).rowcount
        scope = f"{lo} ~ {hi} (최근 {args.days}일)"
    c.commit()

    f_after = c.execute("SELECT COUNT(*) FROM findings").fetchone()[0]
    remaining = [r[0] for r in c.execute("SELECT date FROM diary_index ORDER BY date")]
    c.close()

    print(f"삭제 범위 : {scope}")
    print(f"삭제된 행 : {n}")
    print(f"findings  : {f_before} -> {f_after} (보존)")
    print(f"남은 일기 : {remaining}")
    if args.all or args.days > 7:
        print("\n[주의] backfill 기본 룩백은 7일 — 오늘 -7~-1일만 자동 재생성됩니다.")
        print("       더 오래된 날짜까지 재생성하려면 룩백 값을 늘려야 합니다(코드 변경).")
    print("\n이제 앱을 실행하면 backfill이 재생성합니다.")


if __name__ == "__main__":
    main()
