"""First-party two-axis rectangle/advance font. Optional fontTools4.62.1 only."""
from pathlib import Path
from fontTools.fontBuilder import FontBuilder
from fontTools.pens.ttGlyphPen import TTGlyphPen
from fontTools.ttLib.tables.TupleVariation import TupleVariation

def generate(destination):
    builder=FontBuilder(1000,isTTF=True)
    order=['.notdef','space','a','b']
    advances={'.notdef':400,'space':250,'a':500,'b':600}
    builder.setupGlyphOrder(order);builder.setupCharacterMap({32:'space',97:'a',98:'b',65:'a',103:'b',109:'a',48:'b'})
    glyphs={}
    for name in order:
        pen=TTGlyphPen(None)
        if name!='space':
            pen.moveTo((10,0));pen.lineTo((advances[name]-20,0));pen.lineTo((advances[name]-20,700));pen.lineTo((10,700));pen.closePath()
        glyphs[name]=pen.glyph()
    builder.setupGlyf(glyphs);builder.setupHorizontalMetrics({name:(advance,0 if name=="space" else 10) for name,advance in advances.items()})
    builder.setupHorizontalHeader(ascent=900,descent=-200)
    builder.setupNameTable({'familyName':'Ravet Variation','styleName':'Regular','uniqueFontIdentifier':'Ravet Variation v1','fullName':'Ravet Variation Regular','psName':'RavetVariation-Regular','version':'Version 1.0'})
    builder.setupOS2(sTypoAscender=900,sTypoDescender=-200,usWinAscent=900,usWinDescent=200,usWeightClass=400)
    builder.setupPost();builder.setupMaxp()
    builder.setupFvar([('wght',100,400,900,'Weight'),('wdth',50,100,200,'Width')],[{'location':{'wght':400,'wdth':100},'stylename':'Regular'},{'location':{'wght':900,'wdth':100},'stylename':'Heavy'},{'location':{'wght':400,'wdth':200},'stylename':'Wide'}])
    variations={}
    for name in order:
        empty=name=='space'
        tuples=[]
        for axis,side,ink,advance in [('wght',-1,-40,-50),('wght',1,80,100),('wdth',-1,-100,-100),('wdth',1,200,200)]:
            if empty:
                delta=0 if axis=='wght' else advance//2
                points=[(0,0),(delta,0),(0,0),(0,0)]
            else:points=[(0,0),(ink,0),(ink,0),(0,0),(0,0),(advance,0),(0,0),(0,0)]
            region=(-1,-1,0) if side<0 else (0,1,1)
            tuples.append(TupleVariation({axis:region},points))
        variations[name]=tuples
    builder.setupGvar(variations)
    builder.font['head'].created=builder.font['head'].modified=2082844800
    builder.save(destination)

if __name__=='__main__':generate(Path(__file__).with_name('RavetVariation.ttf'))
