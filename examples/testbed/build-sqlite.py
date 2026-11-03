"""Builds testbed.db from sqlite.sql next to this script.

Usage: python build-sqlite.py [output.db]
"""
import pathlib
import sqlite3
import sys

here = pathlib.Path(__file__).resolve().parent
out = pathlib.Path(sys.argv[1]) if len(sys.argv) > 1 else here / "testbed.db"
out.unlink(missing_ok=True)
con = sqlite3.connect(out)
con.executescript((here / "sqlite.sql").read_text(encoding="utf-8"))
con.commit()
tables = [r[0] for r in con.execute("SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name")]
for t in tables:
    n = con.execute(f'SELECT count(*) FROM "{t}"').fetchone()[0]
    print(f"{t:16} {n}")
con.execute("VACUUM")
con.close()
print(f"written: {out}")
