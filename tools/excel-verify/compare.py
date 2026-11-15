"""Compares two cell dumps (dump.js / excel-dump.ps1).

    py -I compare.py <a.json> <b.json> [v] [f] [t] [n]

v = value (Value2; dates as serials), f = formula, t = text shown, n = number format.
Prints each difference and exits with 1 when there is one.
Environment: TOL = relative tolerance for numbers (default 1e-15), SKIP = sheets to leave out (comma-separated).
"""
import json, os, sys

sys.stdout.reconfigure(encoding="utf-8")
a = json.load(open(sys.argv[1], encoding="utf-8-sig"))
b = json.load(open(sys.argv[2], encoding="utf-8-sig"))
what = sys.argv[3:] or ["v"]
TOL = float(os.environ.get("TOL", "1e-15"))
SKIP = set(filter(None, os.environ.get("SKIP", "").split(",")))


def norm(v):
    if isinstance(v, dict) and "d" in v:
        return v["d"]
    return None if v == "" else v


def same(x, y):
    if isinstance(x, bool) or isinstance(y, bool):
        return x == y
    if isinstance(x, (int, float)) and isinstance(y, (int, float)):
        return abs(x - y) <= TOL * max(abs(y), 1e-300)
    return x == y


def fnorm(f):
    # Excel shows @ on formulas saved without dynamic-array metadata.
    return f.replace("=@", "=") if f else None


def same_shown(x, y, nf):
    """What the width of the column decides: a cell filled with # (a date
    that cannot be shown), and the spaces a * in the format repeats."""
    if x and y and set(x) == {"#"} and set(y) == {"#"}:
        return True
    if "*" in nf:
        return "".join(x.split()) == "".join(y.split())
    return False


bad = {k: 0 for k in what}
n = 0
for name, sb in b["sheets"].items():
    if name in SKIP:
        continue
    sa = a["sheets"].get(name, {"cells": {}})["cells"]
    for k in dict.fromkeys(list(sb["cells"]) + list(sa)):
        cb = sb["cells"].get(k) or {}
        ca = sa.get(k) or {}
        n += 1
        for w in what:
            if w == "v":
                x, y = norm(ca.get("v")), norm(cb.get("v"))
                # A date goes through JavaScript's Date: to the millisecond.
                is_date = isinstance(ca.get("v"), dict) or isinstance(cb.get("v"), dict)
                ok = same(x, y) or (is_date and isinstance(x, float) and isinstance(y, float) and abs(x - y) < 0.6 / 86400000)
            elif w == "f":
                x, y = fnorm(ca.get("f")), fnorm(cb.get("f"))
                # xljs gives the formula of an array formula in its first cell only.
                arr = cb.get("arr")
                ok = x == y or (arr is not None and x is None and not arr.startswith(k + ":") and arr != k)
            elif w == "n":
                x, y = ca.get("nf"), cb.get("nf")
                ok = x == y
            else:
                x, y = ca.get("t") or "", cb.get("t") or ""
                ok = x == y or same_shown(x, y, cb.get("nf") or "")
            if not ok:
                bad[w] += 1
                extra = f"  nf={cb.get('nf')!r}" if w == "t" else f"  {cb.get('f') or ''}"
                print(f"[{w}] {name}!{k}: {x!r} / {y!r}{extra}")
print(f"{n} cells, differ: {bad}")
sys.exit(1 if any(bad.values()) else 0)
