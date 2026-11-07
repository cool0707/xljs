"""Make src/api/preset_shapes.xml from presetShapeDefinitions.xml.

The definitions of the preset shapes (ECMA-376 Part 1, the annex
"Preset Shape Definitions"; LibreOffice has a copy at
oox/source/drawingml/customshapes/presetShapeDefinitions.xml). This keeps
what drawing needs (avLst, gdLst, rect, pathLst) without the namespaces
and whitespace.

    python3 tools/preset-shapes/make.py presetShapeDefinitions.xml > src/api/preset_shapes.xml

The text warps of WordArt (the annex "Preset Text Warp Definitions";
LibreOffice: oox/source/export/presetTextWarpDefinitions.xml) the same way:

    python3 tools/preset-shapes/make.py presetTextWarpDefinitions.xml \
        | sed 's/presetShapeDefinitions>/presetTextWarpDefinitions>/' > src/api/preset_text_warps.xml
"""
import sys
import xml.etree.ElementTree as ET

KEEP = {"avLst", "gdLst", "rect", "pathLst"}


def local(tag):
    return tag.rsplit("}", 1)[-1]


def out(e):
    attrs = "".join(f' {k}="{v}"' for k, v in e.attrib.items() if not k.startswith("{"))
    kids = "".join(out(c) for c in e)
    name = local(e.tag)
    return f"<{name}{attrs}>{kids}</{name}>" if kids else f"<{name}{attrs}/>"


root = ET.parse(sys.argv[1]).getroot()
parts = []
for shape in root:
    kids = "".join(out(c) for c in shape if local(c.tag) in KEEP)
    parts.append(f"<{local(shape.tag)}>{kids}</{local(shape.tag)}>")
sys.stdout.write("<presetShapeDefinitions>\n" + "\n".join(parts) + "\n</presetShapeDefinitions>\n")
