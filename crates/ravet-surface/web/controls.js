// Reusable local DOM controls. The consumer owns native transport and errors.
(() => {
  'use strict';
  let owner = null;
  const navigationKeys=new Set(['ArrowUp','ArrowDown','ArrowLeft','ArrowRight','Home','End','PageUp','PageDown']);
  class Control {
    constructor(post) {
      if (typeof post !== 'function') throw new TypeError('Control transport required');
      this.post = post; this.listeners = []; this.disposed = false;
    }
    listen(node, event, handler) {
      node.addEventListener(event, handler);
      this.listeners.push([node, event, handler]);
    }
    send(type, id, value) {
      if (this.disposed) return false;
      return this.post({type, id, value}) !== false;
    }
    dispose() {
      for (const [node,event,handler] of this.listeners) node.removeEventListener(event,handler);
      this.listeners = []; this.disposed = true; return true;
    }
  }
  function parameter(id) {
    if (!Number.isInteger(id) || id < 0 || id > 0xffffffff) throw new TypeError('Invalid parameter ID');
  }
  class Slider extends Control {
    constructor(range, number, {id, minimum, maximum, step='any'}, post) {
      super(post); parameter(id);
      if (range?.type !== 'range' || (number && number.type !== 'number') ||
          !Number.isFinite(minimum) || !Number.isFinite(maximum) || minimum >= maximum ||
          (step !== 'any' && (!Number.isFinite(step) || step <= 0))) throw new TypeError('Invalid slider');
      this.id=id; this.range=range; this.number=number; this.active=false; this.keyboard=false; this.keys=new Set();
      this.minimum=minimum; this.maximum=maximum;
      for (const node of [range,number].filter(Boolean)) {
        node.min=String(minimum); node.max=String(maximum); node.step=String(step);
      }
      this.listen(range,'pointerdown',()=>{this.keyboard=false;this.keys.clear();this.begin()});
      this.listen(range,'keydown',event=> {
        if(navigationKeys.has(event.key) && this.begin()) {
          this.keyboard=true;this.keys.add(event.key);
        }
      });
      this.listen(range,'input',()=> {
        const value=this.value(range.value);
        if (value !== null && this.begin()) {
          if (number) number.value=String(value);
          this.send('value',id,value);
        }
      });
      this.listen(range,'change',()=> {
        if (this.active) { if(!this.keyboard) this.end(); }
        else { const value=this.value(range.value); if(value!==null) this.send('edit',id,value); }
      });
      for (const [node,event] of [[window,'pointerup'],[window,'pointercancel'],[window,'blur'],[range,'blur']]) {
        this.listen(node,event,()=>this.end());
      }
      this.listen(range,'keyup',event=> {
        if(!navigationKeys.has(event.key)) return;
        this.keys.delete(event.key);
        if(this.keyboard && this.keys.size===0) this.end();
      });
      if (number) this.listen(number,'change',()=> {
        const value=this.value(number.value);
        if (value !== null && (!owner || owner.end())) {
          range.value=String(value); number.value=String(value); this.send('edit',id,value);
        }
      });
    }
    value(text) {
      if (String(text).trim()==='') return null;
      const value=Number(text);
      return Number.isFinite(value) ? Math.max(this.minimum,Math.min(this.maximum,value)) : null;
    }
    begin() {
      if (this.disposed) return false;
      if (this.active) return true;
      if (owner && !owner.end()) return false;
      if (!this.send('begin',this.id,0)) return false;
      this.active=true; owner=this; return true;
    }
    end() {
      if (!this.active) return true;
      if (!this.send('end',this.id,0)) return false;
      this.active=false; this.keyboard=false; this.keys.clear(); if(owner===this) owner=null; return true;
    }
    update(value) {
      if (this.disposed || !Number.isFinite(value) || value<this.minimum || value>this.maximum) return false;
      if (this.active || document.activeElement===this.range || document.activeElement===this.number) return false;
      this.range.value=String(value); if(this.number) this.number.value=String(value); return true;
    }
    dispose() { return this.end() && super.dispose(); }
  }
  class Knob extends Control {
    constructor(node, number, {id, minimum, maximum, step=0.01, value=minimum, logarithmic=false}, post) {
      super(post); parameter(id);
      if(node?.nodeType!==1 || (number && number.type!=='number') ||
          !Number.isFinite(minimum) || !Number.isFinite(maximum) || minimum>=maximum ||
          !Number.isFinite(step) || step<=0 || !Number.isFinite(value) || value<minimum || value>maximum ||
          typeof logarithmic!=='boolean' || (logarithmic && minimum<=0)) throw new TypeError('Invalid knob');
      Object.assign(this,{id,node,number,minimum,maximum,step,logarithmic,defaultValue:value});
      this.active=false;this.keys=new Set();this.pointer=null;this.current=value;
      node.tabIndex=0;node.setAttribute('role','slider');
      node.setAttribute('aria-valuemin',String(minimum));node.setAttribute('aria-valuemax',String(maximum));
      if(number){number.min=minimum;number.max=maximum;number.step=step;}
      this.paint(value);
      this.listen(node,'pointerdown',e=>{
        if(e.button!==0 || this.pointer!==null || !this.begin())return;
        e.preventDefault();node.focus();this.pointer=e.pointerId;this.originY=e.clientY;this.origin=this.normalized(this.current);
        try{node.setPointerCapture(e.pointerId)}catch{}
      });
      this.listen(node,'pointermove',e=>{
        if(e.pointerId!==this.pointer)return;
        const n=Math.max(0,Math.min(1,this.origin+(this.originY-e.clientY)/(e.shiftKey?1500:150)));
        this.change(this.physical(n));
      });
      for(const event of ['pointerup','pointercancel','lostpointercapture'])this.listen(node,event,e=>{
        if(e.pointerId===this.pointer)this.end();
      });
      this.listen(window,'pointerup',e=>{if(e.pointerId===this.pointer)this.end()});
      this.listen(window,'pointercancel',e=>{if(e.pointerId===this.pointer)this.end()});
      this.listen(node,'keydown',e=>{
        if(!navigationKeys.has(e.key))return;e.preventDefault();
        if(!this.begin())return;this.keys.add(e.key);
        let value=this.current,increment=this.step*((e.key==='PageUp'||e.key==='PageDown')?10:1);
        if(e.key==='Home')value=minimum;else if(e.key==='End')value=maximum;
        else if(logarithmic){const delta=(e.key==='PageUp'||e.key==='PageDown') ? .1 : (e.shiftKey ? .001 : .01);
          value=this.physical(Math.max(0,Math.min(1,this.normalized(value)+(['ArrowUp','ArrowRight','PageUp'].includes(e.key)?delta:-delta))));}
        else value+=['ArrowUp','ArrowRight','PageUp'].includes(e.key)?increment:-increment;
        this.change(value);
      });
      this.listen(node,'keyup',e=>{this.keys.delete(e.key);if(this.keys.size===0&&this.pointer===null)this.end()});
      for(const target of [window,node])this.listen(target,'blur',()=>this.end());
      this.listen(document,'visibilitychange',()=>{if(document.hidden)this.end()});
      this.listen(node,'dblclick',()=>this.commit(this.defaultValue));
      if(number)this.listen(number,'change',()=>{if(number.value.trim()!=='')this.commit(Number(number.value))});
    }
    normalized(v){return this.logarithmic?Math.log(v/this.minimum)/Math.log(this.maximum/this.minimum):(v-this.minimum)/(this.maximum-this.minimum);}
    physical(n){return this.logarithmic?this.minimum*Math.pow(this.maximum/this.minimum,n):this.minimum+n*(this.maximum-this.minimum);}
    bounded(v){if(!Number.isFinite(v))return null;return Math.max(this.minimum,Math.min(this.maximum,this.minimum+Math.round((v-this.minimum)/this.step)*this.step));}
    paint(v){
      this.current=v;const n=this.normalized(v),digits=Math.max(0,Math.min(10,Math.ceil(-Math.log10(this.step))));
      const text=v.toFixed(digits);this.node.style.setProperty('--angle',`${n*270-135}deg`);
      this.node.style.setProperty('--sweep',`${n*270}deg`);this.node.setAttribute('aria-valuenow',String(v));
      this.node.setAttribute('aria-valuetext',text);if(this.number)this.number.value=text;
    }
    begin(){
      if(this.disposed)return false;if(this.active)return true;
      if(owner&&!owner.end())return false;if(!this.send('begin',this.id,0))return false;
      this.active=true;owner=this;return true;
    }
    change(v){v=this.bounded(v);if(v!==null&&this.send('value',this.id,v))this.paint(v);}
    commit(v){v=this.bounded(v);if(v===null||this.disposed||(owner&&!owner.end()))return false;
      if(!this.send('edit',this.id,v))return false;this.paint(v);return true;}
    end(){
      if(!this.active)return true;if(!this.send('end',this.id,0))return false;
      const pointer=this.pointer;this.pointer=null;this.keys.clear();this.active=false;if(owner===this)owner=null;
      if(pointer!==null){try{this.node.releasePointerCapture(pointer)}catch{}}return true;
    }
    update(v){
      if(this.disposed||!Number.isFinite(v)||v<this.minimum||v>this.maximum||this.active||
          document.activeElement===this.node||document.activeElement===this.number)return false;
      this.paint(v);return true;
    }
    dispose(){return this.end()&&super.dispose();}
  }
  let keyboardSequence=0;
  class PianoKeyboard extends Control {
    constructor(root,{firstNote=48,lastNote=72,onNoteOn,onNoteOff,computerKeys='awsedftgyhujk',computerStart=60}) {
      super(()=>true);
      const black=note=>[1,3,6,8,10].includes(note%12);
      if(root?.nodeType!==1||!Number.isInteger(firstNote)||!Number.isInteger(lastNote)||firstNote<0||lastNote>127||firstNote>=lastNote||black(firstNote)||black(lastNote)||
          typeof onNoteOn!=='function'||typeof onNoteOff!=='function'||typeof computerKeys!=='string'||computerKeys.length>128||!Number.isInteger(computerStart))throw new TypeError('Invalid piano keyboard');
      Object.assign(this,{root,onNoteOn,onNoteOff,firstNote,lastNote});this.enabled=true;this.held=new Map();this.pointers=new Set();this.external=new Set();this.buttons=new Map();this.prefix=`piano-${++keyboardSequence}`;
      root.classList.add('ravet-piano');root.setAttribute('role','group');root.tabIndex=0;root.replaceChildren();
      const whiteCount=Array.from({length:lastNote-firstNote+1},(_,i)=>firstNote+i).filter(n=>!black(n)).length;
      let index=0;
      for(let note=firstNote;note<=lastNote;note++) {
        const isBlack=black(note),button=document.createElement('button');button.type='button';button.className=`ravet-piano-key ${isBlack?'black':'white'}`;button.dataset.note=note;
        const name=['C','C♯','D','D♯','E','F','F♯','G','G♯','A','A♯','B'][note%12]+(Math.floor(note/12)-1);
        button.setAttribute('aria-label',`Play ${name}, MIDI note ${note}`);button.setAttribute('aria-pressed','false');
        button.style.left=`${100*(isBlack?index-.31:index)/whiteCount}%`;button.style.width=`${100*(isBlack?.62:1)/whiteCount}%`;
        if(note%12===0){const label=document.createElement('span');label.textContent=name;button.append(label);}
        if(!isBlack)index++;root.append(button);this.buttons.set(note,button);
        this.listen(button,'keydown',e=>{if([' ','Enter'].includes(e.key)){e.preventDefault();if(!e.repeat)this.down(note,`${this.prefix}-button-${note}`)}});
        this.listen(button,'keyup',e=>{if([' ','Enter'].includes(e.key)){e.preventDefault();this.up(`${this.prefix}-button-${note}`)}});
        this.listen(button,'blur',()=>this.up(`${this.prefix}-button-${note}`));
      }
      const keyAt=target=>{const button=target?.closest?.('[data-note]');return button&&root.contains(button)?Number(button.dataset.note):null;};
      this.listen(root,'pointerdown',e=>{
        const note=keyAt(e.target);if(e.button!==0||note===null||!this.enabled)return;
        e.preventDefault();root.focus();this.pointers.add(e.pointerId);try{root.setPointerCapture(e.pointerId)}catch{}
        this.down(note,`${this.prefix}-pointer-${e.pointerId}`);
      });
      this.listen(root,'pointermove',e=>{
        if(!this.pointers.has(e.pointerId))return;e.preventDefault();
        const note=keyAt(document.elementFromPoint(e.clientX,e.clientY)),source=`${this.prefix}-pointer-${e.pointerId}`;
        if(this.held.get(source)===note)return;this.up(source);if(note!==null)this.down(note,source);
      });
      const end=e=>{this.pointers.delete(e.pointerId);this.up(`${this.prefix}-pointer-${e.pointerId}`);try{root.releasePointerCapture(e.pointerId)}catch{}};
      for(const [node,event] of [[root,'pointerup'],[root,'pointercancel'],[root,'lostpointercapture'],[window,'pointerup'],[window,'pointercancel']])this.listen(node,event,end);
      this.listen(window,'keydown',e=>{
        if(e.repeat||e.altKey||e.ctrlKey||e.metaKey||e.target.closest?.('input,select,textarea,button,[role=slider]'))return;
        if(e.key.length!==1)return;const index=computerKeys.indexOf(e.key.toLowerCase());if(index>=0){e.preventDefault();this.down(computerStart+index,`${this.prefix}-computer-${e.code}`)}
      });
      this.listen(window,'keyup',e=>this.up(`${this.prefix}-computer-${e.code}`));
      this.listen(window,'blur',()=>this.releaseAll());this.listen(document,'visibilitychange',()=>{if(document.hidden)this.releaseAll()});
    }
    down(note,source){
      if(this.disposed||!this.enabled||!this.buttons.has(note)||this.held.has(source))return false;
      if(this.onNoteOn(note,source)===false)return false;this.held.set(source,note);this.paint(note);return true;
    }
    up(source){const note=this.held.get(source);if(note===undefined)return;this.held.delete(source);this.onNoteOff(source);this.paint(note);}
    paint(note){const active=this.external.has(note)||Array.from(this.held.values()).includes(note),button=this.buttons.get(note);button.classList.toggle('held',active);button.setAttribute('aria-pressed',String(active));}
    setActiveNotes(notes){
      if(this.disposed||!Array.isArray(notes)||notes.length>128||notes.some(n=>!Number.isInteger(n)||n<0||n>127))return false;
      this.external=new Set(notes);for(const note of this.buttons.keys())this.paint(note);return true;
    }
    setEnabled(enabled){if(this.disposed||typeof enabled!=='boolean')return false;if(!enabled)this.releaseAll();this.enabled=enabled;for(const button of this.buttons.values())button.disabled=!enabled;return true;}
    releaseAll(){for(const source of Array.from(this.held.keys()))this.up(source);this.pointers.clear();}
    dispose(){this.releaseAll();this.external.clear();for(const note of this.buttons.keys())this.paint(note);return super.dispose();}
  }
  class Toggle extends Control {
    constructor(node,id,post) {
      super(post); parameter(id);
      if(node?.type!=='checkbox') throw new TypeError('Checkbox required');
      this.node=node;
      this.listen(node,'change',()=> { if(!owner || owner.end()) this.send('edit',id,node.checked?1:0); });
    }
    update(checked) {
      if(this.disposed || typeof checked!=='boolean' || document.activeElement===this.node) return false;
      this.node.checked=checked; return true;
    }
  }
  class Choice extends Control {
    constructor(node,id,post) {
      super(post); parameter(id);
      if(node?.tagName!=='SELECT' || node.options.length===0 || node.options.length>256) throw new TypeError('Bounded select required');
      const values=Array.from(node.options,o=>Number(o.value));
      if(values.some(v=>!Number.isFinite(v)) || new Set(values).size!==values.length) throw new TypeError('Unique finite choice values required');
      this.node=node; this.values=new Map(Array.from(node.options,o=>[Number(o.value),o.value]));
      this.listen(node,'change',()=> { const value=Number(node.value); if(this.values.has(value) && (!owner || owner.end())) this.send('edit',id,value); });
    }
    update(value) {
      if(this.disposed || !this.values.has(value) || document.activeElement===this.node) return false;
      this.node.value=this.values.get(value); return true;
    }
  }
  class ActionButton extends Control {
    constructor(node,action,post) {
      super(post);
      if(node?.tagName!=='BUTTON' || typeof action!=='string' || action.length===0 || action.length>128) throw new TypeError('Button/action required');
      this.listen(node,'click',()=> { if(!this.disposed && (!owner || owner.end())) this.post({type:'action',action}); });
    }
  }
  function meterText({peak,rms,nonfinite}) {
    const db=value=>value>0?(20*Math.log10(value)).toFixed(1):'−∞';
    return `Peak ${db(peak)} · RMS ${db(rms)} dB${nonfinite?' · invalid audio':''}`;
  }
  class AccessibleMeter extends Control {
    constructor(node,reading,{label,minimum=0,maximum=1,format=meterText}) {
      super(()=>true);
      if(node?.tagName!=='METER' || reading?.nodeType!==1 || node.ownerDocument!==reading.ownerDocument ||
          typeof label!=='string' || label.trim()==='' || label.length>256 ||
          !Number.isFinite(minimum) || !Number.isFinite(maximum) || minimum<0 || minimum>=maximum ||
          typeof format!=='function') throw new TypeError('Invalid accessible meter');
      this.node=node;this.reading=reading;this.format=format;this.minimum=minimum;this.maximum=maximum;
      node.min=minimum;node.max=maximum;node.setAttribute('aria-label',label);
    }
    update(reading) {
      if(this.disposed) return false;
      const snapshot={peak:reading?.peak,rms:reading?.rms,nonfinite:reading?.nonfinite};
      if(!Number.isFinite(snapshot.peak) || snapshot.peak<0 || !Number.isFinite(snapshot.rms) ||
          snapshot.rms<0 || typeof snapshot.nonfinite!=='boolean') return false;
      let text;
      try { text=this.format(Object.freeze(snapshot)); } catch { return false; }
      if(typeof text!=='string' || text.trim()==='' || text.length>4096) return false;
      this.node.value=Math.max(this.minimum,Math.min(this.maximum,snapshot.rms));
      this.node.setAttribute('aria-valuetext',text);
      if(this.reading.textContent!==text) this.reading.textContent=text;
      return true;
    }
  }
  window.RavetControls=Object.freeze({Slider,Knob,PianoKeyboard,Toggle,Choice,ActionButton,AccessibleMeter});
})();
