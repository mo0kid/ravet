// Synthetic reusable control contract, with no native devices or parameter transport.
(async()=>{
  'use strict';
  const results=[],check=(ok,name)=>{if(!ok)throw new Error(name);results.push(name)};
  const root=document.getElementById('test-fixtures');
  const make=(count=4)=>{const group=document.createElement('div');root.append(group);const nodes=Array.from({length:count},(_,i)=>{const node=document.createElement('button');node.textContent=String(i);group.append(node);return node});return [group,nodes]};
  const press=(node,key,extra={})=>{const event=new KeyboardEvent('keydown',{key,bubbles:true,cancelable:true,...extra});node.dispatchEvent(event);return event.defaultPrevented};
  const settle=()=>new Promise(resolve=>setTimeout(resolve,0));
  try{
    let [group,nodes]=make();nodes[0].setAttribute('tabindex','4');nodes[1].setAttribute('tabindex','-3');
    const focus=new RavetControls.RovingFocus(group,nodes,{index:1});
    check(nodes.map(n=>n.tabIndex).join()==='-1,0,-1,-1','one composite tab stop');
    check(focus.focus(1)&&document.activeElement===nodes[1],'explicit focus');
    nodes[2].disabled=true;check(press(nodes[1],'ArrowRight')&&document.activeElement===nodes[3],'skip disabled');
    check(press(nodes[3],'ArrowRight')&&document.activeElement===nodes[0],'wrap forward');
    check(press(nodes[0],'ArrowLeft')&&document.activeElement===nodes[3],'wrap backward');
    check(press(nodes[3],'Home')&&document.activeElement===nodes[0]&&press(nodes[0],'End')&&document.activeElement===nodes[3],'home and end');
    check(!press(nodes[3],'ArrowUp')&&!press(nodes[3],'ArrowLeft',{shiftKey:true})&&!press(nodes[3],'ArrowLeft',{altKey:true})&&!press(nodes[3],'ArrowLeft',{ctrlKey:true})&&!press(nodes[3],'ArrowLeft',{metaKey:true}),'unowned directions and modifiers');
    nodes[3].hidden=true;nodes[0].setAttribute('aria-disabled','true');focus.refresh();check(nodes[1].tabIndex===0&&!focus.focus(3),'hidden and ARIA disabled');
    group.inert=true;check(focus.refresh().length===0&&nodes.every(n=>n.tabIndex===-1),'inert ancestors');group.inert=false;
    nodes[1].style.visibility='hidden';check(focus.refresh().length===0,'CSS visibility');nodes[1].style.visibility='visible';
    const active=document.activeElement;focus.refresh();check(document.activeElement===active,'refresh does not steal focus');
    check(!focus.focus(-1)&&!focus.focus(0.5)&&!focus.focus(99),'invalid focus preserves order');
    focus.dispose();check(nodes[0].getAttribute('tabindex')==='4'&&nodes[1].getAttribute('tabindex')==='-3'&&!nodes[2].hasAttribute('tabindex')&&focus.refresh().length===0&&!focus.focus(1),'disposal restores original attributes');
    group.remove();
    [group,nodes]=make(3);const vertical=new RavetControls.RovingFocus(group,nodes,{orientation:'vertical',wrap:false});vertical.focus(0);
    check(press(nodes[0],'ArrowUp')&&document.activeElement===nodes[0]&&press(nodes[0],'ArrowDown')&&document.activeElement===nodes[1]&&!press(nodes[1],'ArrowRight'),'vertical bounded edges');vertical.dispose();
    const rtl=new RavetControls.RovingFocus(group,nodes,{rtl:true});rtl.focus(1);check(press(nodes[1],'ArrowRight')&&document.activeElement===nodes[0]&&press(nodes[0],'ArrowLeft')&&document.activeElement===nodes[1],'explicit RTL direction');rtl.dispose();
    const both=new RavetControls.RovingFocus(group,nodes,{orientation:'both'});both.focus(1);check(press(nodes[1],'ArrowUp')&&document.activeElement===nodes[0],'both directions');both.dispose();
    const invalid=[()=>new RavetControls.RovingFocus(group,[]),()=>new RavetControls.RovingFocus(group,[nodes[0],nodes[0]]),()=>new RavetControls.RovingFocus(group,nodes,{index:3}),()=>new RavetControls.RovingFocus(group,nodes,{wrap:1}),()=>new RavetControls.RovingFocus(group,nodes,{orientation:'grid'}),()=>new RavetControls.RovingFocus(group,[document.createElement('button')])];
    check(invalid.every(f=>{try{f();return false}catch{return true}}),'invalid profile rejection');group.remove();
    [group,nodes]=make(256);const max=new RavetControls.RovingFocus(group,nodes);max.focus(0);check(press(nodes[0],'End')&&document.activeElement===nodes[255],'maximum 256 members');max.dispose();const overflow=document.createElement('button');group.append(overflow);let rejected=false;try{new RavetControls.RovingFocus(group,[...nodes,overflow])}catch{rejected=true}check(rejected,'257 members reject');group.remove();
    const pianoRoot=document.createElement('div');pianoRoot.style.width='500px';root.append(pianoRoot);const notes=new Map(),events=[];
    const piano=new RavetControls.PianoKeyboard(pianoRoot,{firstNote:60,lastNote:72,onNoteOn:(n,s)=>{if(n===65)return false;notes.set(s,n);events.push(['on',n]);return true},onNoteOff:s=>{events.push(['off',notes.get(s)]);notes.delete(s)}});
    const key=n=>piano.buttons.get(n),down=n=>key(n).dispatchEvent(new KeyboardEvent('keydown',{key:' ',bubbles:true}));
    check(pianoRoot.tabIndex===-1&&Array.from(piano.buttons.values()).filter(n=>n.tabIndex===0).length===1,'piano one tab stop');
    key(60).focus();down(60);key(60).dispatchEvent(new KeyboardEvent('keydown',{key:'Enter',bubbles:true}));
    key(60).dispatchEvent(new KeyboardEvent('keyup',{key:' ',bubbles:true}));check(notes.size===1&&events.length===1,'overlapping play keys retain note');
    press(key(60),'ArrowRight');check(notes.size===0&&document.activeElement===key(61)&&events.map(e=>e.join(':')).join()==='on:60,off:60','piano focus move releases exact source');
    press(key(61),'Home');check(document.activeElement===key(60)&&press(key(60),'ArrowLeft')&&document.activeElement===key(60),'piano bounds');
    down(60);pianoRoot.hidden=true;await settle();check(notes.size===0&&!piano.down(60,'hidden')&&Array.from(piano.buttons.values()).every(n=>n.tabIndex===-1),'hidden piano release and no new notes');
    pianoRoot.hidden=false;await settle();check(key(60).tabIndex===0,'visible piano restores tab stop');
    key(60).focus();down(60);key(60).disabled=true;await settle();check(notes.size===0&&key(61).tabIndex===0,'individual disabled key release');key(60).disabled=false;
    key(62).focus();down(62);pianoRoot.remove();await settle();check(notes.size===0&&!piano.down(62,'removed'),'removed piano releases held note');root.append(pianoRoot);await settle();
    key(64).focus();down(64);piano.setEnabled(false);check(notes.size===0&&Array.from(piano.buttons.values()).every(n=>n.tabIndex===-1),'disabled piano releases notes');piano.setEnabled(true);
    const denied=events.length;key(65).focus();down(65);key(65).dispatchEvent(new KeyboardEvent('keyup',{key:' ',bubbles:true}));check(events.length===denied&&!piano.held.size,'rejected note has no release');
    key(64).focus();down(64);const count=events.length;piano.dispose();piano.dispose();await settle();check(notes.size===0&&events.length===count+1,'teardown releases once');pianoRoot.remove();
    const detached=document.implementation.createHTMLDocument('no live window'),detachedRoot=detached.createElement('div'),detachedButton=detached.createElement('button');
    detachedButton.setAttribute('tabindex','7');detachedRoot.append(detachedButton);detached.body.append(detachedRoot);const original=detachedRoot.innerHTML;
    let noWindow=false;try{new RavetControls.RovingFocus(detachedRoot,[detachedButton])}catch{noWindow=true}
    check(noWindow&&detachedButton.getAttribute('tabindex')==='7','focus rejects missing live window atomically');
    noWindow=false;try{new RavetControls.PianoKeyboard(detachedRoot,{onNoteOn:()=>true,onNoteOff:()=>{}})}catch{noWindow=true}
    check(noWindow&&detachedRoot.innerHTML===original&&!detachedRoot.hasAttribute('tabindex'),'piano rejects missing live window atomically');
    const frame=document.createElement('iframe');frame.style.cssText='width:600px;height:150px';root.append(frame);
    const child=frame.contentDocument,childWindow=frame.contentWindow,childRoot=child.createElement('div');childRoot.style.cssText='position:relative;width:500px;height:100px';child.body.append(childRoot);
    const childEvents=[],childNotes=new Set(),childPiano=new RavetControls.PianoKeyboard(childRoot,{firstNote:60,lastNote:72,onNoteOn:(note,source)=>{childEvents.push(['on',note,source]);childNotes.add(source);return true},onNoteOff:source=>{childEvents.push(['off',source]);childNotes.delete(source)}});
    window.dispatchEvent(new KeyboardEvent('keydown',{key:'a',code:'KeyA'}));check(childNotes.size===0,'parent keys do not play child document piano');
    childWindow.dispatchEvent(new childWindow.KeyboardEvent('keydown',{key:'a',code:'KeyA'}));check(childNotes.size===1&&childPiano.buttons.get(60).ownerDocument===child,'child document owns keys and created nodes');
    window.dispatchEvent(new Event('blur'));check(childNotes.size===1,'parent blur does not release child notes');
    childWindow.dispatchEvent(new childWindow.Event('blur'));check(childNotes.size===0&&childEvents.length===2&&childEvents[0][2]===childEvents[1][1],'child blur releases original source once');
    childWindow.dispatchEvent(new childWindow.KeyboardEvent('keydown',{key:'a',code:'KeyA'}));Object.defineProperty(child,'hidden',{value:true,configurable:true});child.dispatchEvent(new childWindow.Event('visibilitychange'));check(childNotes.size===0&&childEvents.length===4,'child document visibility releases notes');
    delete child.hidden;childPiano.dispose();childWindow.dispatchEvent(new childWindow.KeyboardEvent('keydown',{key:'a',code:'KeyA'}));check(childEvents.length===4,'child owner listeners removed on disposal');frame.remove();
    window.liveFocus=new RavetControls.RovingFocus(document.getElementById('live-group'),['live-a','live-b','live-c'].map(id=>document.getElementById(id)));
    window.liveNotes=[];window.livePiano=new RavetControls.PianoKeyboard(document.getElementById('live-piano'),{firstNote:60,lastNote:72,onNoteOn:(note,source)=>{liveNotes.push(['on',note,source]);return true},onNoteOff:source=>liveNotes.push(['off',source])});
    window.ravetFocusResults={passed:true,checks:results};document.getElementById('reading').textContent=`PASS ${results.length} focus and piano contracts`;
  }catch(error){window.ravetFocusResults={passed:false,error:String(error),checks:results};document.getElementById('reading').textContent=String(error);}
})();
