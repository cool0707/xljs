"""Lists what a workbook lost when it was opened and saved again.

    py -I partdiff.py <before.xlsx> <after.xlsx>

Parts: the names without their numbers (`xl/slicers/slicer#.xml`), those
fewer after. Elements and attributes: in each kind of part (workbook,
styles, sheets, drawings...), the element names and the attributes of
elements (`sheetView@showZeros`) none of the parts of that kind has any
more. Prints nothing and exits with 0 when nothing was lost.
"""
import collections
import re
import sys
import zipfile

sys.stdout.reconfigure(encoding="utf-8")

# Lost without harm (Excel makes them again, or they are the same written another way).
HARMLESS_PARTS = {"xl/calcChain.xml"}
HARMLESS_ELEMENTS = {"is", "xr:revisionPtr", "xcalcf:calcFeatures", "x15ac:absPath", "mc:AlternateContent", "mc:Choice", "mc:Fallback"}
# Attributes (of any element) lost without harm: the ids for co-authoring, what the markup
# compatibility ignores, how spaces are kept.
HARMLESS_ATTRIBUTES = {"mc:Ignorable", "xml:space", "xr:uid", "xr2:uid", "xr3:uid", "xr6:uid", "xr10:uid", "xr16:uid"}
# By kind of part: the id of the theme's family, the ids shapes are made with (for co-authoring).
HARMLESS_BY_KIND = {
    "xl/theme/theme#.xml": {"a:ext", "a:extLst", "thm15:themeFamily"},
    "xl/drawings/drawing#.xml": {"a16:creationId"},
}


def kind(name):
    return re.sub(r"\d+", "#", name)


def parts(path):
    z = zipfile.ZipFile(path)
    return {n: z.read(n) for n in z.namelist()}


def elements(data):
    try:
        text = data.decode("utf-8")
    except UnicodeDecodeError:
        return set()
    out = set()
    for m in TAG.finditer(text):
        name = m.group(1)
        if name in HARMLESS_ELEMENTS:
            continue
        out.add(name)
        for attr in ATTRIBUTE.findall(m.group(2)):
            if not attr.startswith("xmlns") and attr not in HARMLESS_ATTRIBUTES:
                out.add(f"{name}@{attr}")
    return out


# A start tag and its attributes; an attribute's name.
TAG = re.compile(r"<([A-Za-z][\w.]*(?::[\w.]+)?)((?:\s+[\w.:-]+\s*=\s*(?:\"[^\"]*\"|'[^']*'))*)\s*/?>")
ATTRIBUTE = re.compile(r"([\w.:-]+)\s*=")


before, after = parts(sys.argv[1]), parts(sys.argv[2])
lost = []
count_b = collections.Counter(kind(n) for n in before if n not in HARMLESS_PARTS)
count_a = collections.Counter(kind(n) for n in after)
for k, n in sorted(count_b.items()):
    if count_a[k] < n:
        lost.append(f"part     {k}  {n} -> {count_a[k]}")

by_kind_b, by_kind_a = collections.defaultdict(set), collections.defaultdict(set)
for n, d in before.items():
    if n.endswith((".xml", ".rels", ".vml")):
        by_kind_b[kind(n)] |= elements(d)
for n, d in after.items():
    if n.endswith((".xml", ".rels", ".vml")):
        by_kind_a[kind(n)] |= elements(d)
for k in sorted(by_kind_b):
    if k not in by_kind_a:
        continue  # the part itself is gone: reported above
    gone = sorted(by_kind_b[k] - by_kind_a[k] - HARMLESS_ELEMENTS - HARMLESS_BY_KIND.get(k, set()))
    if gone:
        lost.append(f"element  {k}  {' '.join(gone)}")

print("\n".join(lost))
sys.exit(1 if lost else 0)
