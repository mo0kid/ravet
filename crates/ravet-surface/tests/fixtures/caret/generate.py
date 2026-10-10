"""Regenerate a tiny synthetic ligature fixture using optional fontTools 4.62.1."""
from pathlib import Path
from fontTools.fontBuilder import FontBuilder
from fontTools.pens.ttGlyphPen import TTGlyphPen
from fontTools.feaLib.builder import addOpenTypeFeaturesFromString

def build(path):
    builder=FontBuilder(1000,isTTF=True)
    order=['.notdef','space','f','i','ffi']
    builder.setupGlyphOrder(order)
    builder.setupCharacterMap({32:'space',102:'f',105:'i'})
    glyphs={}
    for name in order:
        pen=TTGlyphPen(None)
        if name!='space':
            width={'.notdef':400,'f':180,'i':280,'ffi':980}[name]
            pen.moveTo((10,0));pen.lineTo((width,0));pen.lineTo((width,700));pen.lineTo((10,700));pen.closePath()
        glyphs[name]=pen.glyph()
    builder.setupGlyf(glyphs)
    builder.setupHorizontalMetrics({'.notdef':(400,0),'space':(250,0),'f':(200,0),'i':(300,0),'ffi':(1000,0)})
    builder.setupHorizontalHeader(ascent=800,descent=-200)
    builder.setupNameTable({'familyName':'RAVET Synthetic Ligature','styleName':'Regular','uniqueFontIdentifier':'RAVET Synthetic Ligature v1','fullName':'RAVET Synthetic Ligature Regular','psName':'RAVETSyntheticLigature-Regular','version':'Version 1.0'})
    builder.setupOS2(sTypoAscender=800,sTypoDescender=-200,usWinAscent=800,usWinDescent=200)
    builder.setupPost();builder.setupMaxp()
    addOpenTypeFeaturesFromString(builder.font,'feature liga { sub f f i by ffi; } liga;')
    builder.font['head'].created=builder.font['head'].modified=2082844800
    builder.save(path)
if __name__=='__main__':build(Path(__file__).with_name('RavetSyntheticLigature.ttf'))
