"""Generate original geometric colour-font fixtures; requires fonttools."""
from pathlib import Path
from fontTools.fontBuilder import FontBuilder
from fontTools.pens.ttGlyphPen import TTGlyphPen
from fontTools.colorLib.builder import buildCOLR, buildCPAL
from fontTools.ttLib.tables.otTables import PaintFormat as P, ExtendMode, CompositeMode


def rectangle(x0, y0, x1, y1):
    pen = TTGlyphPen(None)
    pen.moveTo((x0, y0))
    for point in [(x1, y0), (x1, y1), (x0, y1)]:
        pen.lineTo(point)
    pen.closePath()
    return pen.glyph()


def solid(index, alpha=1):
    return {"Format": P.PaintSolid, "PaletteIndex": index, "Alpha": alpha}


def shape(glyph, paint):
    return {"Format": P.PaintGlyph, "Glyph": glyph, "Paint": paint}


line = {"Extend": ExtendMode.PAD, "ColorStop": [
    {"StopOffset": 0, "PaletteIndex": 0, "Alpha": 1},
    {"StopOffset": 1, "PaletteIndex": 1, "Alpha": 1},
]}
paints = {
    "solid": shape("box", solid(0)),
    "linear": shape("box", {"Format": P.PaintLinearGradient, "ColorLine": line,
        "x0": 100, "y0": 0, "x1": 900, "y1": 0, "x2": 100, "y2": 900}),
    "radial": shape("box", {"Format": P.PaintRadialGradient, "ColorLine": line,
        "x0": 500, "y0": 500, "r0": 0, "x1": 500, "y1": 500, "r1": 400}),
    "sweep": shape("box", {"Format": P.PaintSweepGradient, "ColorLine": line,
        "centerX": 500, "centerY": 500, "startAngle": 0, "endAngle": 360}),
    "transform": {"Format": P.PaintTransform,
        "Transform": {"xx": 0.5, "yx": 0, "xy": 0, "yy": 0.5, "dx": 100, "dy": 50},
        "Paint": shape("box", solid(0))},
    "composite": {"Format": P.PaintComposite, "CompositeMode": CompositeMode.SRC_IN,
        "SourcePaint": shape("box", solid(1, 0.5)),
        "BackdropPaint": shape("left", solid(0))},
    "clip": shape("left", {"Format": P.PaintColrLayers, "Layers": [shape("box", solid(1))]}),
    "legacy": [("box", 0)],
}
names = [".notdef", "box", "left", *paints]
fb = FontBuilder(1000, isTTF=True)
fb.setupGlyphOrder(names)
fb.setupCharacterMap({ord("A") + index: name for index, name in enumerate(paints)})
empty = TTGlyphPen(None).glyph()
fb.setupGlyf({name: rectangle(100, 100, 900, 900) if name == "box" else
    rectangle(100, 100, 500, 900) if name == "left" else empty for name in names})
fb.setupHorizontalMetrics({name: (1000, 0) for name in names})
fb.setupHorizontalHeader(ascent=1000, descent=0)
fb.setupNameTable({"familyName": "Blinc COLR Regression", "styleName": "Regular",
    "uniqueFontIdentifier": "BlincCOLRRegression", "fullName": "Blinc COLR Regression", "psName": "BlincCOLRRegression"})
fb.setupOS2(sTypoAscender=1000, sTypoDescender=0, usWinAscent=1000, usWinDescent=0)
fb.setupPost()
fb.setupMaxp()
fb.font["COLR"] = buildCOLR(paints, glyphMap=fb.font.getReverseGlyphMap(),
    clipBoxes={name: (0, 0, 1000, 1000) for name in paints})
fb.font["CPAL"] = buildCPAL([[(1, 0, 0, 1), (0, 0, 1, 1)]])
fb.font.recalcTimestamp = False
fb.font["head"].created = fb.font["head"].modified = 2082844800
fb.save(Path(__file__).with_name("colrv1.ttf"))
