"""First-party rectangular fallback fixtures; optional fontTools4.62.1 only."""
from pathlib import Path
from fontTools.fontBuilder import FontBuilder
from fontTools.pens.ttGlyphPen import TTGlyphPen
from fontTools.feaLib.builder import addOpenTypeFeaturesFromString

def build(name, cmap, advances, ligature=False):
    builder=FontBuilder(1000,isTTF=True)
    order=['.notdef','space',*advances]
    builder.setupGlyphOrder(order)
    builder.setupCharacterMap({32:'space',**cmap})
    glyphs={}
    metrics={'.notdef':400,'space':250,**advances}
    for glyph in order:
        pen=TTGlyphPen(None)
        if glyph!='space':
            width=max(80,metrics[glyph]-20)
            y=750 if glyph=='acute' else 0
            pen.moveTo((10,y));pen.lineTo((width,y));pen.lineTo((width,y+100 if glyph=='acute' else 700));pen.lineTo((10,y+100 if glyph=='acute' else 700));pen.closePath()
        glyphs[glyph]=pen.glyph()
    builder.setupGlyf(glyphs)
    builder.setupHorizontalMetrics({g:(metrics[g],0) for g in order})
    builder.setupHorizontalHeader(ascent=900,descent=-200)
    builder.setupNameTable({'familyName':name,'styleName':'Regular','uniqueFontIdentifier':name+' v1','fullName':name+' Regular','psName':name.replace(' ',''),'version':'Version 1.0'})
    builder.setupOS2(sTypoAscender=900,sTypoDescender=-200,usWinAscent=900,usWinDescent=200)
    builder.setupPost();builder.setupMaxp()
    if ligature:addOpenTypeFeaturesFromString(builder.font,'feature liga { sub f f i by ffi; } liga;')
    builder.font['head'].created=builder.font['head'].modified=2082844800
    builder.save(Path(__file__).with_name(name.replace(' ','')+'.ttf'))

if __name__=='__main__':
    build('Ravet Fallback Primary',{97:'a',102:'f',105:'i',0x628:'beh'},{'a':500,'f':200,'i':300,'ffi':1000,'beh':450},True)
    build('Ravet Fallback Complement',{97:'a',98:'b',99:'c',0x301:'acute'},{'a':600,'b':700,'c':800,'acute':0})
