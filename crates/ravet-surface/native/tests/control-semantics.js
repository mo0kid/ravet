(async()=>{
'use strict';const {Knob,Slider,AccessibleMeter}=RavetControls,results=[];const outside=document.getElementById('outside');
function assert(ok,message='contract'){if(!ok)throw Error(message);}const wait=()=>new Promise(r=>setTimeout(r,20));
function pair(post=()=>true,{doc=document,attributes='',numberAttributes='',hidden=false}={}){const root=doc.createElement('section');root.innerHTML=`<div aria-label="Gain" tabindex="7" ${attributes} style="width:60px;height:60px"></div><input aria-label="Numeric gain" type="number" ${numberAttributes}>`;root.hidden=hidden;doc.body.append(root);const node=root.firstElementChild,number=root.lastElementChild,knob=new Knob(node,number,{id:2,minimum:0,maximum:1,value:.5,step:.1},post);return {root,node,number,knob,dispose(){assert(knob.dispose());root.remove();}};}
function meter(format){const root=document.createElement('section');root.innerHTML='<span id="old-name">Wrong name</span><meter aria-label="Original" aria-labelledby="old-name"></meter><span>Original reading</span>';document.body.append(root);const node=root.querySelector('meter'),reading=root.lastElementChild,widget=new AccessibleMeter(node,reading,{label:'Output peak',...(format?{format}:{})});return {root,node,reading,widget,dispose(){widget.dispose();root.remove();}};}
async function test(name,work){try{await work();results.push({name,pass:true});}catch(error){results.push({name,pass:false,error:String(error)});}}
const reading={peak:2,rms:.5,nonfinite:false};
for(const kind of ['knob','fader'])await test(`${kind} pointer gestures leave number editing without focus highlights or label activation`,()=>{
const root=document.createElement('label'),messages=[];root.htmlFor=`pointer-number-${kind}`;
root.innerHTML=(kind==='knob'?'<div aria-label="Pointer gain" style="width:60px;height:60px"></div>':'<input aria-label="Pointer gain" type="range">')+`<input id="pointer-number-${kind}" aria-label="Pointer numeric gain" type="number">`;
document.body.append(root);const node=root.firstElementChild,number=root.lastElementChild;
const widget=kind==='knob'?new Knob(node,number,{id:30,minimum:0,maximum:1,value:.5,step:.1},m=>messages.push(m)):new Slider(node,number,{id:30,minimum:0,maximum:1,step:.1},m=>messages.push(m));
const style=document.createElement('style');style.textContent='[aria-label="Pointer gain"]:focus{outline:3px solid red}';document.head.append(style);
if(kind==='fader')widget.update(.5);number.focus();assert(document.activeElement===number);
node.dispatchEvent(new PointerEvent('pointerdown',{button:0,pointerId:91,clientY:100,bubbles:true,cancelable:true}));
assert(document.activeElement!==number&&widget.active,'pointer adjustment must leave numeric editing');
if(kind==='knob'){assert(document.activeElement!==node,'knob must not force focus');node.dispatchEvent(new PointerEvent('pointermove',{pointerId:91,clientY:70}));}
else{node.focus();assert(getComputedStyle(node).outlineStyle==='none','native fader pointer focus outline');node.value='.7';node.dispatchEvent(new Event('input'));}
window.dispatchEvent(new PointerEvent('pointerup',{pointerId:91}));node.click();
assert(!widget.active&&document.activeElement!==number&&document.activeElement!==node&&messages.map(m=>m.type).join(',')==='begin,value,end','drag must end without numeric label activation');
assert(widget.update(.4),'host refresh must resume after drag');
outside.dispatchEvent(new KeyboardEvent('keydown',{key:'Tab',bubbles:true}));assert(!node.hasAttribute('data-ravet-pointer-focus'),'Tab restores styling before entering the control');
number.focus();number.value='.8';number.dispatchEvent(new Event('change'));assert(document.activeElement===number&&messages.at(-1).type==='edit'&&Number(number.value)===.8,'direct numeric entry remains available');
node.focus();node.dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowUp',bubbles:true,cancelable:true}));
if(kind==='fader'){node.value='.9';node.dispatchEvent(new Event('input'));}
assert(!node.hasAttribute('data-ravet-pointer-focus')&&(!document.hasFocus()||getComputedStyle(node).outlineStyle==='solid'),'keyboard restores focus styling');node.dispatchEvent(new KeyboardEvent('keyup',{key:'ArrowUp'}));
assert(widget.dispose()&&!node.hasAttribute('data-ravet-pointer-focus'));root.remove();style.remove();
});
await test('pointer presentation restores consumer attributes only after accepted disposal',()=>{
let accept=false;const f=pair(m=>m.type!=='end'||accept,{attributes:'data-ravet-pointer-focus="consumer"'});
f.node.dispatchEvent(new PointerEvent('pointerdown',{button:0,pointerId:92,clientY:100}));assert(!f.knob.dispose()&&f.node.getAttribute('data-ravet-pointer-focus')==='');
accept=true;f.dispose();assert(f.node.getAttribute('data-ravet-pointer-focus')==='consumer');
});

await test('knob explicit disabling publishes semantic state and removes sequential tab stop',()=>{const f=pair();assert(f.knob.setEnabled(false)&&f.node.getAttribute('aria-disabled')==='true'&&f.node.tabIndex===-1&&f.number.disabled);assert(f.knob.setEnabled(true)&&f.node.tabIndex===0&&!f.number.disabled);f.dispose();});
await test('disabled knob rejects all input paths but preserves unfocused host updates',()=>{const messages=[],f=pair(m=>messages.push(m));f.knob.setEnabled(false);f.node.dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowUp',bubbles:true,cancelable:true}));f.node.dispatchEvent(new PointerEvent('pointerdown',{pointerId:4,button:0,bubbles:true}));f.node.dispatchEvent(new MouseEvent('dblclick',{bubbles:true}));f.number.value='.9';f.number.dispatchEvent(new Event('change'));assert(!f.knob.begin()&&!f.knob.commit(.8)&&!f.knob.change(.8)&&messages.length===0);outside.focus();assert(f.knob.update(.3)&&f.knob.current===.3&&messages.length===0);f.dispose();});
await test('rejected disable retains exact attributes and owner until successful explicit retry',()=>{let accept=false;const messages=[],f=pair(m=>{messages.push(m);return m.type!=='end'||accept;});assert(f.knob.begin());const before=[f.node.getAttribute('aria-disabled'),f.node.getAttribute('tabindex'),f.number.disabled];assert(!f.knob.setEnabled(false)&&f.knob.active&&before.every((v,i)=>v===[f.node.getAttribute('aria-disabled'),f.node.getAttribute('tabindex'),f.number.disabled][i]));const count=messages.length;assert(!f.knob.setEnabled(true)&&!f.knob.begin()&&!f.knob.commit(.9)&&!f.knob.change(.9)&&!f.knob.dispose()&&messages.length===count+1);accept=true;assert(f.knob.end()&&f.knob.setEnabled(false));f.dispose();});
await test('ordinary rejected end blocks further edits and re-enable until accepted retry',()=>{let accept=false;const f=pair(m=>m.type!=='end'||accept);f.knob.begin();assert(!f.knob.end()&&!f.knob.begin()&&!f.knob.change(.8)&&!f.knob.commit(.8)&&!f.knob.setEnabled(true));accept=true;assert(f.knob.end()&&f.knob.begin()&&f.knob.end());f.dispose();});
await test('throwing disable end retains ownership and blocks re-enable',()=>{let fail=true;const f=pair(m=>{if(m.type==='end'&&fail)throw Error('transport');});f.knob.begin();let threw=false;try{f.knob.setEnabled(false);}catch{threw=true;}assert(threw&&f.knob.active&&!f.knob.setEnabled(true));fail=false;assert(f.knob.end());f.dispose();});
await test('external disabling terminates held gesture and removes Tab stop',async()=>{const messages=[],f=pair(m=>messages.push(m));f.knob.begin();f.node.setAttribute('aria-disabled','true');await wait();assert(!f.knob.active&&f.node.tabIndex===-1&&!f.knob.commit(.8)&&messages.filter(m=>m.type==='end').length===1);f.node.removeAttribute('aria-disabled');await wait();assert(f.node.tabIndex===0&&f.knob.refresh());f.dispose();});
await test('external disable rejected end retains owner after availability returns',async()=>{let accept=false;const f=pair(m=>m.type!=='end'||accept);f.knob.begin();f.node.hidden=true;await wait();assert(f.knob.active&&f.node.tabIndex===-1);f.node.hidden=false;await wait();assert(!f.knob.setEnabled(true)&&!f.knob.begin()&&!f.knob.commit(.8));accept=true;assert(f.knob.end()&&f.knob.refresh());f.dispose();});
await test('constructor hidden layout remains enabled when consumer reveals it',()=>{const f=pair(()=>true,{hidden:true});f.root.hidden=false;assert(f.knob.refresh()&&f.knob.begin()&&f.knob.end());f.dispose();});
await test('paired pre-existing native disabled state is retained on re-enable and exact disposal',()=>{const f=pair(()=>true,{numberAttributes:'disabled="consumer"'});f.knob.setEnabled(false);f.knob.setEnabled(true);assert(f.number.disabled&&f.number.getAttribute('disabled')==='consumer');f.dispose();assert(f.node.getAttribute('tabindex')==='7'&&f.node.getAttribute('aria-disabled')===null&&f.number.getAttribute('disabled')==='consumer');});
await test('transport callback cannot reenter same or competing ownership',()=>{const root=document.createElement('input');root.type='range';document.body.append(root);const other=new Slider(root,null,{id:3,minimum:0,maximum:1},()=>true);let f,reentered=[];f=pair(m=>{if(m.type==='begin'||m.type==='value'||m.type==='edit'){reentered.push([f.knob.setEnabled(false),f.knob.dispose(),other.begin()]);}return true;});assert(f.knob.begin()&&f.knob.change(.6)&&f.knob.commit(.7)&&Math.abs(f.knob.current-.7)<1e-12&&reentered.length===3&&reentered.every(a=>a.every(v=>v===false)));f.dispose();other.dispose();root.remove();});
await test('own document removal/adoption suppress edits and preserves exact retirement',async()=>{const frame=document.createElement('iframe');document.body.append(frame);const f=pair();f.knob.begin();frame.contentDocument.body.append(f.node);await wait();assert(!f.knob.active&&!f.knob.begin()&&!f.knob.commit(.8));f.dispose();f.node.remove();frame.remove();});
await test('meter explicit label wins and exact original name references restore',()=>{const f=meter();assert(f.node.getAttribute('aria-label')==='Output peak'&&!f.node.hasAttribute('aria-labelledby')&&f.widget.update(reading)&&f.node.getAttribute('aria-valuetext').includes('6.0'));f.dispose();assert(f.node.getAttribute('aria-label')==='Original'&&f.node.getAttribute('aria-labelledby')==='old-name');});
await test('meter detached/foreign pair rejects atomically',()=>{const doc=document.implementation.createHTMLDocument('dead');doc.body.innerHTML='<meter aria-label="Original"></meter><span>Read</span>';const node=doc.body.firstElementChild,before=node.outerHTML;let rejected=false;try{new AccessibleMeter(node,doc.body.lastElementChild,{label:'Output'});}catch{rejected=true;}assert(rejected&&node.outerHTML===before);});
await test('formatter disposal prevents stale display writes',()=>{let f;f=meter(()=>{f.widget.dispose();return 'stale';});const value=f.node.value,text=f.reading.textContent;assert(!f.widget.update(reading)&&f.node.value===value&&f.reading.textContent===text&&f.node.getAttribute('aria-label')==='Original');f.dispose();});
await test('formatter removal/adoption prevents stale display writes',()=>{let f;f=meter(()=>{f.reading.remove();return 'stale';});const value=f.node.value;assert(!f.widget.update(reading)&&f.node.value===value&&!f.node.hasAttribute('aria-valuetext'));f.dispose();});
await test('formatter recursive update rejects and publishes only outer snapshot',()=>{let f,nested;f=meter(()=>{nested=f.widget.update(reading);return 'outer';});assert(f.widget.update(reading)&&nested===false&&f.reading.textContent==='outer');f.dispose();});
await test('consumer knob tabindex is restored on enable and exact original on disposal',()=>{const f=pair();f.node.setAttribute('tabindex','3');f.knob.setEnabled(false);f.knob.setEnabled(true);assert(f.node.getAttribute('tabindex')==='3');f.dispose();assert(f.node.getAttribute('tabindex')==='7');});
await test('accepted begin retirement closes or retains the accepted host gesture',()=>{
for(const mutation of ['remove','adopt','disable','pair-disable']){let f;const messages=[];f=pair(m=>{messages.push(m);if(m.type==='begin'){if(mutation==='remove')f.node.remove();else if(mutation==='adopt')document.implementation.createHTMLDocument('adopt').body.append(f.node);else if(mutation==='disable')f.node.setAttribute('aria-disabled','true');else f.number.disabled=true;}return true;});assert(!f.knob.begin()&&!f.knob.active&&messages.map(m=>m.type).join(',')==='begin,end');f.dispose();f.node.remove();}
let accept=false,f;f=pair(m=>{if(m.type==='begin')f.node.hidden=true;return m.type!=='end'||accept;});assert(!f.knob.begin()&&f.knob.active&&f.knob.pendingEnd&&!f.knob.commit(.9)&&!f.knob.setEnabled(true));accept=true;assert(f.knob.end());f.dispose();
});
await test('accepted value and edit retirement never paint a stale pair',()=>{
for(const type of ['value','edit'])for(const mutation of ['remove','adopt','disable','pair-disable']){let f;const host=[];f=pair(m=>{host.push(m);if(m.type===type){if(mutation==='remove')f.node.remove();else if(mutation==='adopt')document.implementation.createHTMLDocument('adopt').body.append(f.number);else if(mutation==='disable')f.node.setAttribute('aria-disabled','true');else f.number.disabled=true;}return true;});if(type==='value')assert(f.knob.begin());const previous=f.node.getAttribute('aria-valuenow');assert(!(type==='value'?f.knob.change(.8):f.knob.commit(.8))&&host.some(m=>m.type===type&&m.value===.8)&&f.knob.current===.5&&f.node.getAttribute('aria-valuenow')===previous);assert(!f.knob.active);f.dispose();f.node.remove();f.number.remove();}
});
await test('accepted disable end retirement leaves subsequent owned attributes untouched',()=>{let f;f=pair(m=>{if(m.type==='end')document.implementation.createHTMLDocument('adopt').body.append(f.number);return true;});f.knob.begin();const before=f.node.getAttribute('aria-disabled');assert(!f.knob.setEnabled(false)&&!f.knob.active&&f.node.getAttribute('aria-disabled')===before&&!f.knob.update(.8)&&!f.knob.setEnabled(true));f.dispose();f.number.remove();});
await test('host update accepts a healthy disabled pair and rejects retired original ownership',()=>{const f=pair();f.knob.setEnabled(false);outside.focus();assert(f.knob.update(.4));document.implementation.createHTMLDocument('adopt').body.append(f.node);const before=f.node.getAttribute('aria-valuenow');assert(!f.knob.update(.8)&&!f.knob.setEnabled(true)&&f.node.getAttribute('aria-valuenow')===before);f.dispose();f.node.remove();});
await test('own end callback retirement prevents a later host edit',()=>{for(const mutation of ['remove','adopt','disable']){let f;const messages=[];f=pair(m=>{messages.push(m);if(m.type==='end'){if(mutation==='remove')f.node.remove();else if(mutation==='adopt')document.implementation.createHTMLDocument('adopt').body.append(f.number);else f.node.setAttribute('aria-disabled','true');}return true;});assert(f.knob.begin()&&!f.knob.commit(.8)&&messages.map(m=>m.type).join(',')==='begin,end'&&f.knob.current===.5);f.dispose();f.node.remove();f.number.remove();}});
await test('competing owner end callback retirement prevents a later host begin or edit',()=>{for(const request of ['begin','commit'])for(const mutation of ['remove','adopt','disable']){let f;const input=document.createElement('input');input.type='range';document.body.append(input);const other=new Slider(input,null,{id:4,minimum:0,maximum:1},m=>{if(m.type==='end'){if(mutation==='remove')f.node.remove();else if(mutation==='adopt')document.implementation.createHTMLDocument('adopt').body.append(f.number);else f.node.setAttribute('aria-disabled','true');}return true;});const messages=[];f=pair(m=>messages.push(m));assert(other.begin()&&!(request==='begin'?f.knob.begin():f.knob.commit(.8))&&messages.length===0&&!other.active);f.dispose();f.node.remove();f.number.remove();other.dispose();input.remove();}});
await test('retired refresh recovers original gesture without writing foreign presentation',()=>{const f=pair();f.knob.begin();document.implementation.createHTMLDocument('adopt').body.append(f.node);const before=f.node.outerHTML;assert(!f.knob.refresh()&&!f.knob.active&&f.node.outerHTML===before);f.dispose();f.node.remove();});
await test('all twelve controls retain native or explicit labelled semantic states',async()=>{
const root=document.createElement('section');root.innerHTML=`
<label for="audit-range">Volume</label><input id="audit-range" type="range"><div id="audit-knob" aria-label="Pan" style="width:60px;height:60px"></div>
<label for="audit-toggle">Bypass</label><input id="audit-toggle" type="checkbox"><label for="audit-choice">Mode</label><select id="audit-choice"><option value="0">Clean</option><option value="1">Drive</option></select><button type="button" id="audit-action">Reset</button>
<meter id="audit-meter"></meter><span id="audit-reading"></span><div id="audit-piano" role="group" aria-label="Piano"></div>
<div id="audit-focus" role="toolbar" aria-label="Tools"><button type="button">One</button><button type="button">Two</button></div>
<textarea id="audit-text" aria-label="Notes"></textarea><button type="button" id="audit-trigger">Presets</button><div id="audit-menu"><button type="button">Warm</button></div>
<div id="audit-tabs"><button type="button" id="audit-tab">Main</button></div><section id="audit-panel">Main contents</section>
<dialog id="audit-dialog"><input aria-label="Setting"></dialog>`;document.body.append(root);const q=id=>root.querySelector('#audit-'+id),widgets=[];
const slider=new RavetControls.Slider(q('range'),null,{id:10,minimum:0,maximum:2},()=>true);widgets.push(slider);slider.update(1);
const knob=new RavetControls.Knob(q('knob'),null,{id:11,minimum:-1,maximum:1,value:.25},()=>true);widgets.push(knob);
const toggle=new RavetControls.Toggle(q('toggle'),12,()=>true);widgets.push(toggle);toggle.update(true);
const choice=new RavetControls.Choice(q('choice'),13,()=>true);widgets.push(choice);choice.update(1);
widgets.push(new RavetControls.ActionButton(q('action'),'reset',()=>true));
const meter=new RavetControls.AccessibleMeter(q('meter'),q('reading'),{label:'Output'});widgets.push(meter);meter.update(reading);
const piano=new RavetControls.PianoKeyboard(q('piano'),{firstNote:60,lastNote:62,onNoteOn:()=>true,onNoteOff:()=>{}});widgets.push(piano);piano.setActiveNotes([60]);
widgets.push(new RavetControls.RovingFocus(q('focus'),[...q('focus').children]));widgets.push(new RavetControls.TextEditor(q('text')));
widgets.push(new RavetControls.PopupMenu(q('trigger'),q('menu'),[{node:q('menu').firstElementChild,action:'warm'}],{label:'Presets',onAction:()=>{}}));
widgets.push(new RavetControls.TabbedPanels(q('tabs'),[{tab:q('tab'),panel:q('panel')}],{label:'Sections'}));
const modal=new RavetControls.ModalDialog(q('dialog'),{label:'Settings',initialFocus:q('dialog').firstElementChild});widgets.push(modal);
assert(widgets.length===12&&q('range').type==='range'&&q('range').min==='0'&&q('range').max==='2'&&q('range').value==='1');
assert(q('knob').getAttribute('role')==='slider'&&q('knob').getAttribute('aria-label')==='Pan'&&q('knob').getAttribute('aria-valuenow')==='0.25');
assert(q('toggle').checked&&q('choice').value==='1'&&q('meter').getAttribute('aria-label')==='Output'&&q('meter').getAttribute('aria-valuetext').includes('6.0'));
assert(q('piano').querySelector('[aria-pressed="true"]').getAttribute('aria-label').includes('MIDI note 60'));
assert([...q('focus').children].filter(n=>n.tabIndex===0).length===1&&q('text').getAttribute('aria-label')==='Notes');
assert(q('trigger').getAttribute('aria-expanded')==='false'&&q('menu').getAttribute('role')==='menu'&&q('menu').firstElementChild.getAttribute('role')==='menuitem');
assert(q('tabs').getAttribute('role')==='tablist'&&q('tab').getAttribute('aria-selected')==='true'&&q('panel').getAttribute('aria-labelledby')==='audit-tab'&&!q('panel').hidden);
assert(modal.open()&&q('dialog').matches(':modal')&&q('dialog').getAttribute('aria-modal')==='true');modal.close();await wait();
for(const w of widgets)w.dispose();for(let i=0;i<150&&modal.isDraining();i++)await wait();assert(!modal.isDraining());root.remove();
});
window.ravetSemanticResults={pass:results.every(r=>r.pass),count:results.length,results,hasFocus:document.hasFocus()};document.getElementById('report').textContent=JSON.stringify(window.ravetSemanticResults,null,2);
})();
window.ravetSemanticTrusted={messages:[],keys:[]};
const enabledRoot=document.createElement('section');enabledRoot.innerHTML='<div id="semantic-enabled" aria-label="Enabled gain" style="width:60px;height:60px"></div><input id="semantic-number" aria-label="Enabled numeric gain" type="number">';document.body.append(enabledRoot);
ravetSemanticTrusted.enabled=new RavetControls.Knob(enabledRoot.firstElementChild,enabledRoot.lastElementChild,{id:21,minimum:0,maximum:1,value:.5,step:.1},m=>{ravetSemanticTrusted.messages.push(m);return true});
const disabledRoot=document.createElement('section');disabledRoot.innerHTML='<div id="semantic-disabled" aria-label="Disabled gain" style="width:60px;height:60px"></div><input id="semantic-disabled-number" aria-label="Disabled numeric gain" type="number">';document.body.append(disabledRoot);
ravetSemanticTrusted.disabled=new RavetControls.Knob(disabledRoot.firstElementChild,disabledRoot.lastElementChild,{id:22,minimum:0,maximum:1,value:.5,step:.1},m=>{ravetSemanticTrusted.messages.push(m);return true});ravetSemanticTrusted.disabled.setEnabled(false);
const after=document.createElement('button');after.id='semantic-after';after.type='button';after.textContent='After disabled controls';document.body.append(after);
document.addEventListener('keyup',event=>{if(ravetSemanticTrusted.keys.length<16)ravetSemanticTrusted.keys.push({key:event.key,trusted:event.isTrusted,active:document.activeElement.id});});
// The native probe drives real AppKit mouse events against these shared controls.
window.ravetPointerNative={
  setup(kind){
    const root=document.createElement('label');root.htmlFor='native-pointer-number';
    root.style.cssText='position:fixed;top:20px;left:20px;z-index:99999;background:white;padding:20px;display:flex;gap:30px;align-items:center';
    root.innerHTML=(kind==='knob'?'<div aria-label="Native gain" style="width:80px;height:80px;background:#ccc;touch-action:none"></div>':'<input aria-label="Native gain" type="range" style="width:250px">')+'<input id="native-pointer-number" aria-label="Native numeric gain" type="number">';
    document.body.append(root);const node=root.firstElementChild,number=root.lastElementChild,messages=[],options={id:40,minimum:0,maximum:1,step:.01,value:.5};
    const widget=kind==='knob'?new RavetControls.Knob(node,number,options,m=>messages.push(m)):new RavetControls.Slider(node,number,options,m=>messages.push(m));
    widget.update(.5);Object.assign(this,{root,node,number,widget,messages});number.focus();
    const r=node.getBoundingClientRect();return {x:r.x+r.width/2,y:r.y+r.height/2,focused:document.activeElement===number};
  },
  started(){return this.widget.active&&document.activeElement!==this.number&&getComputedStyle(this.node).outlineStyle==='none';},
  finished(){
    const types=this.messages.map(m=>m.type),value=Number(this.number.value);
    const pass=!this.widget.active&&document.activeElement!==this.number&&document.activeElement!==this.node&&
      types.includes('begin')&&types.includes('value')&&types.includes('end')&&value>.5&&this.widget.update(.4);
    return {pass,types,value,focused:document.activeElement.id};
  },
  editing(){
    this.number.focus();this.number.value='.8';this.number.dispatchEvent(new Event('change'));
    const numeric=document.activeElement===this.number&&this.messages.at(-1).type==='edit';
    this.node.style.outline='3px solid red';this.node.focus();
    this.node.dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowUp',cancelable:true,bubbles:true}));
    this.node.dispatchEvent(new KeyboardEvent('keyup',{key:'ArrowUp'}));
    const keyboard=!this.node.hasAttribute('data-ravet-pointer-focus')&&getComputedStyle(this.node).outlineStyle==='solid';
    this.widget.dispose();this.root.remove();return numeric&&keyboard;
  }
};
