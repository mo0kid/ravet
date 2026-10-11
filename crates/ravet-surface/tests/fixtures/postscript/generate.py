#!/usr/bin/env python3
"""Generate solely first-party analytic CFF fixtures; FontTools is build tooling."""
from pathlib import Path
import hashlib, json, tempfile
from fontTools.fontBuilder import FontBuilder
from fontTools.misc.psCharStrings import T2CharString
from fontTools.cffLib import SubrsIndex
from fontTools.designspaceLib import DesignSpaceDocument, AxisDescriptor, SourceDescriptor
from fontTools.varLib import build

ROOT = Path(__file__).resolve().parent

def create(path, width=100, family='Ravet Analytic Postscript', matrix=None, subr=False):
    fb = FontBuilder(1000, isTTF=False)
    order = ['.notdef', 'space', 'A', 'B']
    fb.setupGlyphOrder(order)
    fb.setupCharacterMap({32: 'space', 65:'A', 66:'B'})
    curve = [0,100,width,100,width,0,'rrcurveto',width,0,width,-100,0,-100,'rrcurveto',-4*width,0,'rlineto']
    programs = {'.notdef':['endchar'], 'space':['endchar'],
                'A':[50,0,'rmoveto']+curve+['endchar'],
                'B':[0,0,'rmoveto',100,200,100,-200,-200,0,'rlineto','endchar']}
    chars = {name:T2CharString(program=program) for name,program in programs.items()}
    info = {'FullName':family+' Regular','FamilyName':family,'Weight':'Regular'}
    if matrix is not None: info['FontMatrix'] = matrix
    fb.setupCFF(family.replace(' ','')+'-Regular', info, chars, {"defaultWidthX":600+4*(width-100),"nominalWidthX":0})
    if subr:
        top=fb.font['CFF '].cff.topDictIndex[0]
        top.Private.Subrs = SubrsIndex()
        top.Private.Subrs.append(T2CharString(program=curve[:7]+['return'],private=top.Private,globalSubrs=top.GlobalSubrs))
        top.GlobalSubrs.append(T2CharString(program=curve[7:]+['return'],private=top.Private,globalSubrs=top.GlobalSubrs))
        chars['A'].program=[50,0,'rmoveto',-107,'callsubr',-107,'callgsubr','endchar']
    fb.setupHorizontalMetrics({name:(600+4*(width-100),0) for name in order})
    fb.setupHorizontalHeader(ascent=800,descent=-200)
    fb.setupNameTable({'familyName':family,'styleName':'Regular','uniqueFontIdentifier':family+' First Party','fullName':family+' Regular','psName':family.replace(' ','')+'-Regular','version':'Version 1.0'})
    fb.setupOS2(sTypoAscender=800,sTypoDescender=-200,usWinAscent=800,usWinDescent=200)
    fb.setupPost()
    fb.setupMaxp()
    fb.font.recalcTimestamp=False
    fb.font['head'].created=fb.font['head'].modified=2082844800
    fb.save(path)

create(ROOT/'analytic-cff.otf')
create(ROOT/'subr-cff.otf',subr=True)
create(ROOT/'matrix-cff.otf',matrix=[.001,0,.00025,.001,0,0])
with tempfile.TemporaryDirectory(prefix='ravet-cff-masters-') as tmp:
    d=DesignSpaceDocument();axis=AxisDescriptor();axis.name='Weight';axis.tag='wght';axis.minimum=100;axis.default=400;axis.maximum=900
    # Deliberately authored nonlinear avar: source user 650 -> normalized .8.
    axis.map=[(100,100),(400,400),(650,800),(900,900)]
    d.addAxis(axis)
    for weight,width in [(100,60),(400,100),(900,180)]:
        path=Path(tmp)/f'master-{weight}.otf';create(path,width,family='Ravet Analytic Variable Postscript')
        source=SourceDescriptor();source.path=str(path);source.name=f'master{weight}';source.familyName='Ravet Analytic Variable Postscript';source.styleName=str(weight);source.location={'Weight':weight}
        if weight==400:source.copyInfo=source.copyLib=source.copyFeatures=True
        d.addSource(source)
    vf,_,_=build(d)
    vf.recalcTimestamp=False;vf['head'].created=vf['head'].modified=2082844800
    vf.save(ROOT/'analytic-cff2.otf')
from fontTools.ttLib import TTFont
for suffix,sign in [('tiny-positive',1),('tiny-negative',-1)]:
    font=TTFont(ROOT/'analytic-cff.otf');font.recalcTimestamp=False
    top=font['CFF '].cff.topDictIndex[0]
    top.CharStrings['A'].setProgram([0,0,'rmoveto',sign*.1,sign*.2,sign*.2,sign*.3,sign*.3,sign*.1,'rrcurveto','endchar'])
    top.FontBBox=[min(0,sign),min(0,sign),max(0,sign),max(0,sign)]
    font['head'].xMin=font['head'].yMin=min(0,sign);font['head'].xMax=font['head'].yMax=max(0,sign)
    font.save(ROOT/(suffix+'-cff.otf'))
# CID-keyed actual FDArray/FDSelect sources, including exact own metadata cap.
from fontTools.cffLib import FontDict, FDArrayIndex, FDSelect
import copy
for count in [2,256,257]:
    font=TTFont(ROOT/'analytic-cff.otf');font.recalcTimestamp=False
    top=font['CFF '].cff.topDictIndex[0]
    private=top.Private
    top.ROS=('Adobe','Identity',0);top.CIDCount=4
    arr=FDArrayIndex();arr.strings=top.strings;arr.GlobalSubrs=top.GlobalSubrs
    for i in range(count):
        fd=FontDict(strings=top.strings,GlobalSubrs=top.GlobalSubrs)
        fd.FontName='RavetFirstPartyFD'+str(i);fd.Private=copy.deepcopy(private);arr.append(fd)
    select=FDSelect(format=3);select.gidArray=[0,0,1,1]
    top.FDArray=arr;top.FDSelect=select
    chars=top.CharStrings
    names=['.notdef','cid00001','cid00002','cid00003']
    chars.charStrings=dict(zip(names,[chars.charStrings[n] for n in top.charset]))
    chars.fdSelect=select;chars.fdArray=arr
    top.charset=names
    if 'Private' in top.rawDict:del top.rawDict['Private']
    if 'Private' in top.__dict__:del top.__dict__['Private']
    font.setGlyphOrder(names)
    for cmap in font['cmap'].tables:
        if cmap.isUnicode():cmap.cmap={32:names[1],65:names[2],66:names[3]}
    font.save(ROOT/('cid-'+str(count)+'-cff.otf'))
# Exact whole-output command boundary: nonclosing diagonal segments are not
# removed by the backend's degenerate/redundant closing-line filter.
for count in [4094,4095]:
    font=TTFont(ROOT/'analytic-cff.otf');font.recalcTimestamp=False
    top=font['CFF '].cff.topDictIndex[0]
    top.CharStrings['A'].setProgram([0,0,'rmoveto']+[v for _ in range(count)for v in (1,1,'rlineto')]+['endchar'])
    top.FontBBox=[0,0,count,count]
    font['head'].xMax=font['head'].yMax=count
    font.save(ROOT/('commands-'+str(count+2)+'-cff.otf'))
# Deliberately malformed first-party source metadata; save raw tables so the
# font writer cannot normalize the exact invalid FDSelect coverage away.
from fontTools.ttLib.tables.DefaultTable import DefaultTable
for label, mutation in [
    ('first-gap', lambda b,o: b.__setitem__(slice(o+3,o+5), bytes([0,1]))),
    ('bad-fd', lambda b,o: b.__setitem__(o+5,2)),
    ('bad-sentinel', lambda b,o: b.__setitem__(slice(o+9,o+11),bytes([0,3]))),
    ('duplicate-first', lambda b,o: b.__setitem__(slice(o+6,o+8),bytes([0,0]))),
]:
    font=TTFont(ROOT/'cid-2-cff.otf');font.recalcTimestamp=False
    top=font['CFF '].cff.topDictIndex[0]
    raw=bytearray(font.reader['CFF ']);offset=top.rawDict['FDSelect']
    assert raw[offset:offset+11]==bytes([3,0,2,0,0,0,0,2,1,0,4])
    mutation(raw,offset)
    table=DefaultTable('CFF ');table.data=bytes(raw);font['CFF ']=table
    font.save(ROOT/('invalid-select-'+label+'.otf'))
for label,matrix in [
    ('valid',[.001,0,0,.001,0,0]),
    ('decimal-near',[.0010000000000001,0,0,.0010000000000001,0,0]),
    ('decimal-far',[.00100000000001,0,0,.00100000000001,0,0]),
    ('shear',[.001,0,.00025,.001,0,0]),
    ('translation',[.001,0,0,.001,1,0]),
    ('unequal',[.001,0,0,.002,0,0]),
]:
    font=TTFont(ROOT/'analytic-cff2.otf');font.recalcTimestamp=False
    font['CFF2'].cff.topDictIndex[0].FontMatrix=matrix
    # FontTools intentionally uses eight significant digits for ordinary
    # float encoding. These two boundary fixtures require exact authored BCD.
    import fontTools.cffLib as cffLib
    original=cffLib.encodeNumber
    spelling={'decimal-near':'0.0010000000000001','decimal-far':'0.00100000000001'}.get(label)
    def exact_number(value):
        if spelling is None or value!=matrix[0]:return original(value)
        nibbles=[int(c) if c.isdigit() else 10 for c in spelling]+[15]
        if len(nibbles)%2:nibbles.append(15)
        return bytes([30]+[(nibbles[i]<<4)|nibbles[i+1]for i in range(0,len(nibbles),2)])
    cffLib.encodeNumber=exact_number
    try:font.save(ROOT/('cff2-matrix-'+label+'.otf'))
    finally:cffLib.encodeNumber=original
# Actual private CFF2 hint blends use the authored table's two regions.
font=TTFont(ROOT/'analytic-cff2.otf');font.recalcTimestamp=False
private=font['CFF2'].cff.topDictIndex[0].FDArray[0].Private
private.BlueValues=[[-10,0,0],[0,0,0],[700,10,20],[710,10,20]]
font.save(ROOT/'private-blend-cff2.otf')
# Odd BlueValues cardinality is malformed source metadata despite unhinted draw.
font=TTFont(ROOT/'analytic-cff.otf');font.recalcTimestamp=False
font['CFF '].cff.topDictIndex[0].Private.BlueValues=[0]
font.save(ROOT/'invalid-private-bluevalues-cff.otf')
font=TTFont(ROOT/'tiny-negative-cff.otf');font.recalcTimestamp=False
font['CFF '].cff.topDictIndex[0].FontMatrix=[.001,0,.00075,.001,0,0]
font.save(ROOT/'tiny-negative-matrix-cff.otf')
# The normal sample intentionally exercises glyph aliases from the same authored
# outline source, rather than relying on installed font bytes in browser fixtures.
for source,name in [('analytic-cff.otf','consumer-cff.otf'),('analytic-cff2.otf','consumer-cff2.otf'),('commands-4097-cff.otf','consumer-capacity-cff.otf')]:
 font=TTFont(ROOT/source);font.recalcTimestamp=False
 for cmap in font['cmap'].tables:
  if cmap.isUnicode():cmap.cmap.update({97:'A',103:'B',109:'A',48:'B'})
 font.save(ROOT/name)
font=TTFont(ROOT/'analytic-cff.otf');font.recalcTimestamp=False
for cmap in font['cmap'].tables:
 if cmap.isUnicode():cmap.cmap={66:'B'}
font.save(ROOT/'fallback-only-b-cff.otf')
font=TTFont(ROOT/'analytic-cff.otf');font.recalcTimestamp=False
extra=DefaultTable('CFF2');extra.data=TTFont(ROOT/'analytic-cff2.otf').reader['CFF2'];font['CFF2']=extra
font.save(ROOT/'ambiguous-cff-and-cff2.otf')
font=TTFont(ROOT/'analytic-cff.otf');font.recalcTimestamp=False;del font['CFF ']
font.save(ROOT/'no-outlines.otf')
font=TTFont(ROOT.parent/'variation/RavetVariation.ttf');font.recalcTimestamp=False
extra=DefaultTable('CFF ');extra.data=bytes([1,0,4,4]);font['CFF ']=extra
font.save(ROOT/'glyf-with-invalid-cff.ttf')
records=[]
for p in sorted([*ROOT.glob('*.otf'),*ROOT.glob('*.ttf')]):
    records.append({'name':p.name,'sha256':hashlib.sha256(p.read_bytes()).hexdigest(),'bytes':p.stat().st_size})
(ROOT/'provenance.json').write_text(json.dumps({'source':'first-party analytic outlines only; no system/proprietary font assets','license':'GPL-3.0-only','generator':'generate.py','fonts':records},indent=2)+'\n')
print(records)
