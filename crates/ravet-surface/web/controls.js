// Reusable local DOM controls. The consumer owns native transport and errors.
(() => {
  'use strict';
  const owners = new WeakMap();
  function liveDocument(node, paired=null) {
    const doc=node?.ownerDocument, view=doc?.defaultView;
    if(node?.nodeType!==1 || !view || view.closed || !node.isConnected ||
        (paired&&(paired.ownerDocument!==doc||!paired.isConnected)))
      throw new TypeError('Connected controls in one live document required');
    return doc;
  }
  function finishOwner(doc) { const owner=owners.get(doc);return !owner||owner.end(); }
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
  function pointerPresentation(control, node, number) {
    const attribute='data-ravet-pointer-focus', original=node.getAttribute(attribute);
    let pointer=null;
    control.listen(node,'pointerdown',event=>{
      if(event.button!==undefined&&event.button!==0)return;
      pointer=event.pointerId;node.setAttribute(attribute,'');
      // Leave explicit text/keyboard editing before starting a pointer gesture.
      const focused=control.document.activeElement;
      if(focused===number||focused===node)focused.blur();
    });
    control.listen(control.document,'keydown',()=>node.removeAttribute(attribute));
    // A surrounding label must not activate its number input after a drag/click.
    control.listen(node,'click',event=>event.preventDefault());
    for(const event of ['pointerup','pointercancel'])control.listen(control.view,event,e=>{
      if(pointer===null||e.pointerId!==pointer)return;pointer=null;
      if(control.document.activeElement===node)node.blur();
    });
    return ()=>{if(original===null)node.removeAttribute(attribute);else node.setAttribute(attribute,original);};
  }
  class Slider extends Control {
    constructor(range, number, {id, minimum, maximum, step='any'}, post) {
      super(post); parameter(id);
      if (range?.type !== 'range' || (number && number.type !== 'number') ||
          !Number.isFinite(minimum) || !Number.isFinite(maximum) || minimum >= maximum ||
          (step !== 'any' && (!Number.isFinite(step) || step <= 0))) throw new TypeError('Invalid slider');
      this.document=liveDocument(range,number);this.view=this.document.defaultView;
      this.id=id; this.range=range; this.number=number; this.active=false; this.keyboard=false; this.keys=new Set();
      this.minimum=minimum; this.maximum=maximum;
      for (const node of [range,number].filter(Boolean)) {
        node.min=String(minimum); node.max=String(maximum); node.step=String(step);
      }
      this.restorePointerPresentation=pointerPresentation(this,range,number);
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
      for (const [node,event] of [[this.view,'pointerup'],[this.view,'pointercancel'],[this.view,'blur'],[range,'blur']]) {
        this.listen(node,event,()=>this.end());
      }
      this.listen(this.document,'visibilitychange',()=>{if(this.document.hidden)this.end();});
      this.listen(range,'keyup',event=> {
        if(!navigationKeys.has(event.key)) return;
        this.keys.delete(event.key);
        if(this.keyboard && this.keys.size===0) this.end();
      });
      if (number) this.listen(number,'change',()=> {
        const value=this.value(number.value);
        if (value !== null && finishOwner(this.document)) {
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
      if (!finishOwner(this.document)) return false;
      if (!this.send('begin',this.id,0)) return false;
      this.active=true; owners.set(this.document,this); return true;
    }
    end() {
      if (!this.active) return true;
      if (!this.send('end',this.id,0)) return false;
      this.active=false; this.keyboard=false; this.keys.clear(); if(owners.get(this.document)===this) owners.delete(this.document); return true;
    }
    update(value) {
      if (this.disposed || !Number.isFinite(value) || value<this.minimum || value>this.maximum) return false;
      if (this.active || this.document.activeElement===this.range || this.document.activeElement===this.number) return false;
      this.range.value=String(value); if(this.number) this.number.value=String(value); return true;
    }
    dispose() { if(this.disposed)return true;if(!this.end())return false;super.dispose();this.restorePointerPresentation();return true; }
  }
  class Knob extends Control {
    constructor(node, number, {id, minimum, maximum, step=0.01, value=minimum, logarithmic=false}, post) {
      super(post); parameter(id);
      if(node?.nodeType!==1 || (number && number.type!=='number') ||
          !Number.isFinite(minimum) || !Number.isFinite(maximum) || minimum>=maximum ||
          !Number.isFinite(step) || step<=0 || !Number.isFinite(value) || value<minimum || value>maximum ||
          typeof logarithmic!=='boolean' || (logarithmic && minimum<=0)) throw new TypeError('Invalid knob');
      this.document=liveDocument(node,number);this.view=this.document.defaultView;
      Object.assign(this,{id,node,number,minimum,maximum,step,logarithmic,defaultValue:value});
      this.active=false;this.keys=new Set();this.pointer=null;this.current=value;
      this.transition=false;this.pendingEnd=false;this.enabled=true;
      this.originalAttributes=[[node,'tabindex',node.getAttribute('tabindex')],[node,'aria-disabled',node.getAttribute('aria-disabled')]];
      if(number)this.originalAttributes.push([number,'disabled',number.getAttribute('disabled')]);
      this.enabledTabindex='0';this.retiredTabindex=null;this.originalNumberDisabled=number?.getAttribute('disabled')??null;
      node.tabIndex=0;node.setAttribute('role','slider');
      node.setAttribute('aria-valuemin',String(minimum));node.setAttribute('aria-valuemax',String(maximum));
      if(number){number.min=minimum;number.max=maximum;number.step=step;}
      this.paint(value);
      this.restorePointerPresentation=pointerPresentation(this,node,number);
      this.listen(node,'pointerdown',e=>{
        if(e.button!==0 || this.pointer!==null || !this.begin())return;
        e.preventDefault();this.pointer=e.pointerId;this.originY=e.clientY;this.origin=this.normalized(this.current);
        try{node.setPointerCapture(e.pointerId)}catch{}
      });
      this.listen(node,'pointermove',e=>{
        if(e.pointerId!==this.pointer)return;
        const n=Math.max(0,Math.min(1,this.origin+(this.originY-e.clientY)/(e.shiftKey?1500:150)));
        if(this.canEdit())this.change(this.physical(n));
      });
      for(const event of ['pointerup','pointercancel','lostpointercapture'])this.listen(node,event,e=>{
        if(e.pointerId===this.pointer)this.end();
      });
      this.listen(this.view,'pointerup',e=>{if(e.pointerId===this.pointer)this.end()});
      this.listen(this.view,'pointercancel',e=>{if(e.pointerId===this.pointer)this.end()});
      this.listen(node,'keydown',e=>{
        if(!navigationKeys.has(e.key)||!this.canEdit())return;e.preventDefault();
        if(!this.begin())return;this.keys.add(e.key);
        let value=this.current,increment=this.step*((e.key==='PageUp'||e.key==='PageDown')?10:1);
        if(e.key==='Home')value=minimum;else if(e.key==='End')value=maximum;
        else if(logarithmic){const delta=(e.key==='PageUp'||e.key==='PageDown') ? .1 : (e.shiftKey ? .001 : .01);
          value=this.physical(Math.max(0,Math.min(1,this.normalized(value)+(['ArrowUp','ArrowRight','PageUp'].includes(e.key)?delta:-delta))));}
        else value+=['ArrowUp','ArrowRight','PageUp'].includes(e.key)?increment:-increment;
        this.change(value);
      });
      this.listen(node,'keyup',e=>{this.keys.delete(e.key);if(this.keys.size===0&&this.pointer===null)this.end()});
      for(const target of [this.view,node])this.listen(target,'blur',()=>this.end());
      this.listen(this.document,'visibilitychange',()=>{if(this.document.hidden)this.end()});
      this.listen(node,'dblclick',()=>this.commit(this.defaultValue));
      if(number)this.listen(number,'change',()=>{if(!number.disabled&&!number.readOnly&&!number.closest('[hidden],[inert],[aria-disabled="true"]')&&number.value.trim()!=='')this.commit(Number(number.value))});
      this.observer=new this.view.MutationObserver(()=>this.refresh());
      this.observer.observe(this.document,{subtree:true,childList:true,attributes:true,attributeFilter:['disabled','hidden','inert','aria-disabled','style','class']});
      if(node.matches(':disabled')||node.closest('[aria-disabled="true"]')){this.enabled=false;node.tabIndex=-1;node.setAttribute('aria-disabled','true');if(number)number.disabled=true;}

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
    validPair(){return !this.view.closed&&this.node.isConnected&&this.node.ownerDocument===this.document&&
      (!this.number||(this.number.isConnected&&this.number.ownerDocument===this.document));}
    available(){return this.validPair()&&(!this.number||!this.number.disabled||this.originalNumberDisabled!==null)&&!this.node.matches(':disabled')&&
      !this.node.closest('[hidden],[inert],[aria-disabled="true"]')&&this.view.getComputedStyle(this.node).visibility==='visible'&&this.node.getClientRects().length>0;}
    postEligible(){return !this.disposed&&this.enabled&&!this.pendingEnd&&this.available();}
    canEdit(){return !this.transition&&this.postEligible();}
    begin(){
      if(!this.canEdit())return false;if(this.active)return true;
      this.transition=true;let accepted=false;
      try{if(!finishOwner(this.document)||!this.postEligible())return false;owners.set(this.document,this);
        if(!this.send('begin',this.id,0))return false;this.active=true;accepted=true;
        if(!this.postEligible()){this.finishGesture();return false;}return true;
      }finally{if(!accepted&&owners.get(this.document)===this)owners.delete(this.document);this.transition=false;}
    }
    change(v){
      if(!this.canEdit())return false;v=this.bounded(v);if(v===null)return false;
      this.transition=true;try{if(!this.send('value',this.id,v))return false;
        if(!this.postEligible()){if(this.active)this.finishGesture();return false;}this.paint(v);return true;}finally{this.transition=false;}
    }
    commit(v){
      if(!this.canEdit()||(v=this.bounded(v))===null)return false;
      this.transition=true;
      try{if((this.active&&!this.finishGesture())||!this.postEligible()||!finishOwner(this.document)||!this.postEligible())return false;owners.set(this.document,this);
        if(!this.send('edit',this.id,v)||!this.postEligible())return false;this.paint(v);return true;
      }finally{if(!this.active&&owners.get(this.document)===this)owners.delete(this.document);this.transition=false;}
    }
    end(){
      if(this.transition)return false;if(!this.active){this.pendingEnd=false;return true;}
      this.transition=true;
      try{return this.finishGesture();}finally{this.transition=false;}
    }
    finishGesture(){
        this.pendingEnd=true;
        if(!this.send('end',this.id,0))return false;
        const pointer=this.pointer;this.pointer=null;this.keys.clear();this.active=false;this.pendingEnd=false;
        if(owners.get(this.document)===this)owners.delete(this.document);
        if(pointer!==null){try{this.node.releasePointerCapture(pointer)}catch{}}return true;
    }
    setEnabled(enabled){
      if(this.disposed||this.transition||!this.validPair()||typeof enabled!=='boolean'||(enabled&&this.pendingEnd))return false;
      if(!enabled){
        this.pendingEnd=this.active;
        if(!this.end()||!this.validPair())return false;
        if(this.enabled)this.enabledTabindex=this.retiredTabindex?this.retiredTabindex.value:this.node.getAttribute('tabindex');this.retiredTabindex=null;
        this.enabled=false;this.node.setAttribute('aria-disabled','true');this.node.tabIndex=-1;if(this.number)this.number.disabled=true;
      }else{
        if(this.retiredTabindex){this.enabledTabindex=this.retiredTabindex.value;this.retiredTabindex=null;}
        this.enabled=true;this.node.setAttribute('aria-disabled','false');
        if(this.enabledTabindex===null)this.node.removeAttribute('tabindex');else this.node.setAttribute('tabindex',this.enabledTabindex);
        if(this.number){if(this.originalNumberDisabled===null)this.number.removeAttribute('disabled');else this.number.setAttribute('disabled',this.originalNumberDisabled);}
      }
      return true;
    }
    refresh(){
      if(this.disposed||this.transition)return false;
      if(!this.validPair()){this.pendingEnd=this.active;try{this.end();}catch{}return false;}
      if(!this.available()){
        if(this.enabled&&!this.retiredTabindex){this.retiredTabindex={value:this.node.getAttribute('tabindex')};this.node.tabIndex=-1;}
        this.pendingEnd=this.active;
        try{this.end();}catch{} // Rejected/throwing end retains original ownership for explicit retry.
        return false;
      }
      if(this.retiredTabindex){if(this.retiredTabindex.value===null)this.node.removeAttribute('tabindex');else this.node.setAttribute('tabindex',this.retiredTabindex.value);this.retiredTabindex=null;}
      return this.enabled&&!this.pendingEnd;
    }
    update(v){
      if(this.disposed||this.transition||!this.validPair()||!Number.isFinite(v)||v<this.minimum||v>this.maximum||this.active||
          this.document.activeElement===this.node||this.document.activeElement===this.number)return false;
      this.paint(v);return true;
    }
    dispose(){
      if(this.transition||!this.end())return false;if(this.disposed)return true;
      this.observer.disconnect();super.dispose();
      this.restorePointerPresentation();
      for(const [node,name,value] of this.originalAttributes){if(value===null)node.removeAttribute(name);else node.setAttribute(name,value);}
      this.originalAttributes=[];return true;
    }
  }
  // Roving tab order for a consumer-labelled composite, without activation or edits.
  class RovingFocus extends Control {
    constructor(root, members, {orientation='horizontal', wrap=true, rtl=false, index=0, onFocus=null}={}) {
      super(()=>true);
      if(root?.nodeType!==1 || !root.ownerDocument.defaultView || root.ownerDocument.defaultView.closed || !Array.isArray(members) || members.length===0 || members.length>256 ||
          !['horizontal','vertical','both'].includes(orientation) || typeof wrap!=='boolean' || typeof rtl!=='boolean' || (onFocus!==null&&typeof onFocus!=='function') ||
          !Number.isInteger(index) || index<0 || index>=members.length || new Set(members).size!==members.length ||
          members.some(node=>node?.nodeType!==1 || typeof node.focus!=='function' || node===root ||
            node.ownerDocument!==root.ownerDocument || !root.contains(node)) ||
          members.some((node,i)=>members.some((other,j)=>i!==j&&node.contains(other)))) throw new TypeError('Invalid focus group');
      Object.assign(this,{root,members:members.slice(),orientation,wrap,rtl,index,onFocus});
      this.original=this.members.map(node=>node.getAttribute('tabindex'));
      this.listen(root,'focusin',event=>{
        const index=this.members.findIndex(node=>node===event.target);
        if(index>=0 && this.eligible(this.members[index])){this.select(index);}
      });
      this.listen(root,'keydown',event=>{
        if(event.defaultPrevented || event.altKey || event.ctrlKey || event.metaKey || event.shiftKey ||
            !this.members.includes(event.target))return;
        const horizontal=this.orientation!=='vertical',vertical=this.orientation!=='horizontal';
        let delta=0;
        if(horizontal && event.key==='ArrowRight')delta=this.rtl?-1:1;
        if(horizontal && event.key==='ArrowLeft')delta=this.rtl?1:-1;
        if(vertical && event.key==='ArrowDown')delta=1;
        if(vertical && event.key==='ArrowUp')delta=-1;
        if(!delta && event.key!=='Home' && event.key!=='End')return;
        const eligible=this.refresh();if(!eligible.length)return;
        const current=eligible.indexOf(this.members.indexOf(event.target));
        let next=event.key==='Home'?0:event.key==='End'?eligible.length-1:current+delta;
        next=this.wrap?(next+eligible.length)%eligible.length:Math.max(0,Math.min(eligible.length-1,next));
        event.preventDefault();this.focus(eligible[next]);
      });
      this.refresh();
    }
    eligible(node){
      const view=this.root.ownerDocument.defaultView;
      return Boolean(view && !view.closed && this.root.isConnected && this.root.contains(node) && !node.matches(':disabled') &&
        !node.closest('[hidden],[inert],[aria-disabled="true"]') && node.getClientRects().length>0 &&
        view.getComputedStyle(node).visibility==='visible');
    }
    refresh(){
      if(this.disposed)return [];
      const eligible=[];for(let i=0;i<this.members.length;i++)if(this.eligible(this.members[i]))eligible.push(i);
      if(!eligible.includes(this.index))this.index=eligible[0]??-1;
      this.members.forEach((node,i)=>node.tabIndex=i===this.index?0:-1);
      return eligible;
    }
    select(index){const changed=this.index!==index;this.index=index;this.refresh();if(changed&&this.onFocus)this.onFocus(this.members[index]);}
    focus(index){
      if(this.disposed || !Number.isInteger(index) || index<0 || index>=this.members.length || !this.eligible(this.members[index]))return false;
      const node=this.members[index];node.focus({preventScroll:true});
      if(node.ownerDocument.activeElement!==node)return false;
      this.select(index);return true;
    }
    dispose(){
      if(this.disposed)return true;
      super.dispose();this.members.forEach((node,i)=>{const value=this.original[i];if(value===null)node.removeAttribute('tabindex');else node.setAttribute('tabindex',value)});
      return true;
    }
  }
  let keyboardSequence=0;
  class PianoKeyboard extends Control {
    constructor(root,{firstNote=48,lastNote=72,onNoteOn,onNoteOff,computerKeys='awsedftgyhujk',computerStart=60}) {
      super(()=>true);
      const black=note=>[1,3,6,8,10].includes(note%12);
      if(root?.nodeType!==1||!root.ownerDocument.defaultView||root.ownerDocument.defaultView.closed||!Number.isInteger(firstNote)||!Number.isInteger(lastNote)||firstNote<0||lastNote>127||firstNote>=lastNote||black(firstNote)||black(lastNote)||
          typeof onNoteOn!=='function'||typeof onNoteOff!=='function'||typeof computerKeys!=='string'||computerKeys.length>128||!Number.isInteger(computerStart))throw new TypeError('Invalid piano keyboard');
      const doc=root.ownerDocument,win=doc.defaultView;
      Object.assign(this,{root,onNoteOn,onNoteOff,firstNote,lastNote});this.enabled=true;this.held=new Map();this.buttonKeys=new Map();this.pointers=new Set();this.external=new Set();this.buttons=new Map();this.prefix=`piano-${++keyboardSequence}`;
      root.classList.add('ravet-piano');root.setAttribute('role','group');root.tabIndex=-1;root.replaceChildren();
      const whiteCount=Array.from({length:lastNote-firstNote+1},(_,i)=>firstNote+i).filter(n=>!black(n)).length;
      let index=0;
      for(let note=firstNote;note<=lastNote;note++) {
        const isBlack=black(note),button=doc.createElement('button');button.type='button';button.className=`ravet-piano-key ${isBlack?'black':'white'}`;button.dataset.note=note;
        const name=['C','C♯','D','D♯','E','F','F♯','G','G♯','A','A♯','B'][note%12]+(Math.floor(note/12)-1);
        button.setAttribute('aria-label',`Play ${name}, MIDI note ${note}`);button.setAttribute('aria-pressed','false');
        button.style.left=`${100*(isBlack?index-.31:index)/whiteCount}%`;button.style.width=`${100*(isBlack?.62:1)/whiteCount}%`;
        if(note%12===0){const label=doc.createElement('span');label.textContent=name;button.append(label);}
        if(!isBlack)index++;root.append(button);this.buttons.set(note,button);
        this.listen(button,'keydown',e=>{if([' ','Enter'].includes(e.key)){
          e.preventDefault();if(e.repeat)return;
          const source=`${this.prefix}-button-${note}`,keys=this.buttonKeys.get(note)||new Set();
          if(this.held.has(source)||this.down(note,source)){keys.add(e.key);this.buttonKeys.set(note,keys);}
        }});
        this.listen(button,'keyup',e=>{if([' ','Enter'].includes(e.key)){
          e.preventDefault();const keys=this.buttonKeys.get(note);if(!keys)return;keys.delete(e.key);
          if(keys.size===0)this.up(`${this.prefix}-button-${note}`);
        }});
        this.listen(button,'blur',()=>this.up(`${this.prefix}-button-${note}`));
      }
      this.focusGroup=new RovingFocus(root,Array.from(this.buttons.values()),{wrap:false,onFocus:node=>{
        for(const [source,note] of Array.from(this.held))if(source===`${this.prefix}-button-${note}` && node!==this.buttons.get(note))this.up(source);
      }});
      // Ancestor visibility and removal can suppress blur in some browser engines.
      this.observer=new win.MutationObserver(()=>this.refresh());
      this.observer.observe(root.ownerDocument.documentElement,{subtree:true,childList:true,attributes:true,
        attributeFilter:['hidden','inert','disabled','aria-disabled','style','class']});
      this.listen(root.ownerDocument,'focusin',event=>{
        for(const [source,note] of Array.from(this.held))if(source===`${this.prefix}-button-${note}` && event.target!==this.buttons.get(note))this.up(source);
      });
      const keyAt=target=>{const button=target?.closest?.('[data-note]');return button&&root.contains(button)?Number(button.dataset.note):null;};
      this.listen(root,'pointerdown',e=>{
        const note=keyAt(e.target);if(e.button!==0||note===null||!this.enabled)return;
        e.preventDefault();root.focus();this.pointers.add(e.pointerId);try{root.setPointerCapture(e.pointerId)}catch{}
        this.down(note,`${this.prefix}-pointer-${e.pointerId}`);
      });
      this.listen(root,'pointermove',e=>{
        if(!this.pointers.has(e.pointerId))return;e.preventDefault();
        const note=keyAt(doc.elementFromPoint(e.clientX,e.clientY)),source=`${this.prefix}-pointer-${e.pointerId}`;
        if(this.held.get(source)===note)return;this.up(source);if(note!==null)this.down(note,source);
      });
      const end=e=>{this.pointers.delete(e.pointerId);this.up(`${this.prefix}-pointer-${e.pointerId}`);try{root.releasePointerCapture(e.pointerId)}catch{}};
      for(const [node,event] of [[root,'pointerup'],[root,'pointercancel'],[root,'lostpointercapture'],[win,'pointerup'],[win,'pointercancel']])this.listen(node,event,end);
      this.listen(win,'keydown',e=>{
        if(e.repeat||e.altKey||e.ctrlKey||e.metaKey||e.target.closest?.('input,select,textarea,button,[role=slider]'))return;
        if(e.key.length!==1)return;const index=computerKeys.indexOf(e.key.toLowerCase());if(index>=0){e.preventDefault();this.down(computerStart+index,`${this.prefix}-computer-${e.code}`)}
      });
      this.listen(win,'keyup',e=>this.up(`${this.prefix}-computer-${e.code}`));
      this.listen(win,'blur',()=>this.releaseAll());this.listen(doc,'visibilitychange',()=>{if(doc.hidden)this.releaseAll()});
    }
    down(note,source){
      if(this.disposed||!this.enabled||!this.buttons.has(note)||!this.focusGroup.eligible(this.buttons.get(note))||this.held.has(source))return false;
      if(this.onNoteOn(note,source)===false)return false;this.held.set(source,note);this.paint(note);return true;
    }
    up(source){const note=this.held.get(source);if(note===undefined)return;this.held.delete(source);if(source===`${this.prefix}-button-${note}`)this.buttonKeys.delete(note);this.onNoteOff(source);this.paint(note);}
    paint(note){const active=this.external.has(note)||Array.from(this.held.values()).includes(note),button=this.buttons.get(note);button.classList.toggle('held',active);button.setAttribute('aria-pressed',String(active));}
    setActiveNotes(notes){
      if(this.disposed||!Array.isArray(notes)||notes.length>128||notes.some(n=>!Number.isInteger(n)||n<0||n>127))return false;
      this.external=new Set(notes);for(const note of this.buttons.keys())this.paint(note);return true;
    }
    setEnabled(enabled){if(this.disposed||typeof enabled!=='boolean')return false;if(!enabled)this.releaseAll();this.enabled=enabled;for(const button of this.buttons.values())button.disabled=!enabled;this.focusGroup.refresh();return true;}
    refresh(){if(this.disposed)return false;if(!this.root.isConnected)this.releaseAll();for(const [source,note] of Array.from(this.held))if(!this.focusGroup.eligible(this.buttons.get(note)))this.up(source);this.focusGroup.refresh();return true;}
    releaseAll(){for(const source of Array.from(this.held.keys()))this.up(source);for(const id of this.pointers){try{this.root.releasePointerCapture(id)}catch{}}this.pointers.clear();}
    dispose(){this.observer.disconnect();this.focusGroup.dispose();this.releaseAll();this.external.clear();for(const note of this.buttons.keys())this.paint(note);return super.dispose();}
  }
  class Toggle extends Control {
    constructor(node,id,post) {
      super(post); parameter(id);
      if(node?.type!=='checkbox') throw new TypeError('Checkbox required');
      this.document=liveDocument(node);this.node=node;
      this.listen(node,'change',()=> { if(finishOwner(this.document)) this.send('edit',id,node.checked?1:0); });
    }
    update(checked) {
      if(this.disposed || typeof checked!=='boolean' || this.document.activeElement===this.node) return false;
      this.node.checked=checked; return true;
    }
  }
  class Choice extends Control {
    constructor(node,id,post) {
      super(post); parameter(id);
      if(node?.tagName!=='SELECT' || node.options.length===0 || node.options.length>256) throw new TypeError('Bounded select required');
      const values=Array.from(node.options,o=>Number(o.value));
      if(values.some(v=>!Number.isFinite(v)) || new Set(values).size!==values.length) throw new TypeError('Unique finite choice values required');
      this.document=liveDocument(node);this.node=node; this.values=new Map(Array.from(node.options,o=>[Number(o.value),o.value]));
      this.listen(node,'change',()=> { const value=Number(node.value); if(this.values.has(value) && finishOwner(this.document)) this.send('edit',id,value); });
    }
    update(value) {
      if(this.disposed || !this.values.has(value) || this.document.activeElement===this.node) return false;
      this.node.value=this.values.get(value); return true;
    }
  }
  class ActionButton extends Control {
    constructor(node,action,post) {
      super(post);
      if(node?.tagName!=='BUTTON' || typeof action!=='string' || action.length===0 || action.length>128) throw new TypeError('Button/action required');
      this.document=liveDocument(node);
      this.listen(node,'click',()=> { if(!this.disposed && finishOwner(this.document)) this.post({type:'action',action}); });
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
      this.document=liveDocument(node,reading);this.view=this.document.defaultView;
      this.node=node;this.reading=reading;this.format=format;this.minimum=minimum;this.maximum=maximum;
      this.formatting=false;this.nameAttributes=['aria-label','aria-labelledby'].map(name=>[name,node.getAttribute(name)]);
      node.min=minimum;node.max=maximum;node.setAttribute('aria-label',label);node.removeAttribute('aria-labelledby');
    }
    update(reading) {
      if(this.disposed||this.formatting||!this.valid()) return false;
      const snapshot={peak:reading?.peak,rms:reading?.rms,nonfinite:reading?.nonfinite};
      if(!Number.isFinite(snapshot.peak) || snapshot.peak<0 || !Number.isFinite(snapshot.rms) ||
          snapshot.rms<0 || typeof snapshot.nonfinite!=='boolean') return false;
      let text;
      this.formatting=true;try { text=this.format(Object.freeze(snapshot)); } catch { return false; }finally{this.formatting=false;}
      if(this.disposed||!this.valid()||typeof text!=='string' || text.trim()==='' || text.length>4096) return false;
      this.node.value=Math.max(this.minimum,Math.min(this.maximum,snapshot.rms));
      this.node.setAttribute('aria-valuetext',text);
      if(this.reading.textContent!==text) this.reading.textContent=text;
      return true;
    }
    valid(){return !this.view.closed&&this.node.isConnected&&this.reading.isConnected&&
      this.node.ownerDocument===this.document&&this.reading.ownerDocument===this.document;}
    dispose(){if(this.disposed)return true;super.dispose();for(const [name,value] of this.nameAttributes){
      if(value===null)this.node.removeAttribute(name);else this.node.setAttribute(name,value);}
      this.nameAttributes=[];return true;}
  }
  // Bounded textarea ownership; browser grapheme data, not Rust EditRevision.
  class TextEditorError extends Error {
    constructor(code) { super(code); this.name='TextEditorError'; this.code=code; }
  }
  class TextEditor {
    constructor(node,{maximumBytes=4096,history=64,maximumRevision=Number.MAX_SAFE_INTEGER,onEdit=null}={}) {
      const doc=node?.ownerDocument,view=doc?.defaultView;
      if(!view || view.closed || !(node instanceof view.HTMLTextAreaElement) || !node.isConnected ||
          !Number.isInteger(maximumBytes) || maximumBytes<1 || maximumBytes>4096 ||
          !Number.isInteger(history) || history<0 || history>64 ||
          !Number.isSafeInteger(maximumRevision) || maximumRevision<1 ||
          (onEdit!==null && typeof onEdit!=='function') || typeof view.Intl?.Segmenter!=='function')
        throw new TypeError('Invalid live textarea profile');
      this.node=node;this.document=doc;this.view=view;this.maximumBytes=maximumBytes;this.historyLimit=history;
      this.maximumRevision=maximumRevision;this.onEdit=onEdit;this.segmenter=new view.Intl.Segmenter(undefined,{granularity:'grapheme'});
      const analysis=this.analyze(node.value),selection=this.nativeSelection(analysis);
      this.state=Object.freeze({value:node.value,selection:Object.freeze(selection)});
      this.token=Object.freeze({sequence:0});this.undoStates=[];this.redoStates=[];this.listeners=[];
      this.busy=false;this.composition=false;this.compositionTimer=null;this.pendingHistory=null;this.disposed=false;this.error=null;this.nativeDispatch=false;this.callbackFailed=false;
      this.originalMaximum=node.getAttribute('maxlength');node.maxLength=maximumBytes;
      this.apply(analysis);
      const listen=(target,type,handler)=>{target.addEventListener(type,handler);this.listeners.push([target,type,handler]);};
      listen(node,'beforeinput',event=>this.native(()=>this.beforeInput(event)));
      listen(node,'input',event=>this.native(()=>this.input(event)));
      listen(node,'select',()=>this.native(()=>this.selectionOnly()));
      listen(node,'keyup',()=>this.native(()=>this.selectionOnly()));
      listen(doc,'selectionchange',()=>{if(doc.activeElement===node)this.native(()=>this.selectionOnly());});
      listen(node,'keydown',event=>this.native(()=>this.keydown(event)));
      listen(node,'compositionstart',()=>this.native(()=>{this.selectionOnly();if(!this.disposed&&!this.composition)this.composition=true;}));
      listen(node,'compositionend',()=>{
        if(this.disposed||!this.composition)return;
        if(this.compositionTimer!==null)view.clearTimeout(this.compositionTimer);
        this.compositionTimer=view.setTimeout(()=>this.native(()=>{this.compositionTimer=null;this.composition=false;this.reconcileInput();}),0);
      });
      listen(node,'blur',()=>{this.cancelPending();});
      this.observer=new view.MutationObserver(()=>{
        if(this.disposed)return;
        if(!node.isConnected||node.ownerDocument!==doc){this.dispose();return;}
        if((this.composition||this.pendingHistory)&&!this.editable())this.cancelPending();
      });
      this.observer.observe(doc,{childList:true,subtree:true,attributes:true,attributeFilter:['disabled','readonly','hidden','inert','style','class','aria-disabled']});
    }
    analyze(value) {
      if(typeof value!=='string')throw new TextEditorError('InvalidInput');
      if(value.length>this.maximumBytes)throw new TextEditorError('Capacity');
      const byteAt=new Int32Array(value.length+1);byteAt.fill(-1);byteAt[0]=0;let bytes=0;
      for(let index=0;index<value.length;) {
        const code=value.codePointAt(index);
        if((code>=0xd800&&code<=0xdfff) || (code<32&&code!==10) || (code>=0x7f&&code<=0x9f) ||
            (code>=0xfdd0&&code<=0xfdef) || (code&0xffff)>=0xfffe)throw new TextEditorError('InvalidInput');
        bytes+=code<0x80?1:code<0x800?2:code<0x10000?3:4;
        if(bytes>this.maximumBytes)throw new TextEditorError('Capacity');
        index+=code>0xffff?2:1;byteAt[index]=bytes;
      }
      const boundaries=[];
      for(const segment of this.segmenter.segment(value)) {
        if(byteAt[segment.index]<0)throw new TextEditorError('InvalidInput');
        boundaries.push({utf16:segment.index,byte:byteAt[segment.index]});
      }
      boundaries.push({utf16:value.length,byte:bytes});return {value,boundaries,bytes};
    }
    nativeSelection(analysis) {
      const start=this.node.selectionStart,end=this.node.selectionEnd;
      const lower=index=>analysis.boundaries.filter(b=>b.utf16<=index).at(-1);
      const upper=index=>analysis.boundaries.find(b=>b.utf16>=index);
      const a=start===end?upper(start):lower(start),b=upper(end);
      if(!a||!b)throw new TextEditorError('InvalidInput');
      return this.node.selectionDirection==='backward'?{anchor:b.byte,focus:a.byte}:{anchor:a.byte,focus:b.byte};
    }
    validateSelection(selection,analysis) {
      if(!selection || !Number.isInteger(selection.anchor) || !Number.isInteger(selection.focus) ||
          !analysis.boundaries.some(b=>b.byte===selection.anchor) || !analysis.boundaries.some(b=>b.byte===selection.focus))
        throw new TextEditorError('InvalidInput');
      return {anchor:selection.anchor,focus:selection.focus};
    }
    apply(analysis=this.analyze(this.state.value)) {
      if(this.node.value!==this.state.value)this.node.value=this.state.value;
      const selection=this.state.selection;
      const anchor=analysis.boundaries.find(b=>b.byte===selection.anchor)?.utf16;
      const focus=analysis.boundaries.find(b=>b.byte===selection.focus)?.utf16;
      const start=Math.min(anchor,focus),end=Math.max(anchor,focus),direction=anchor>focus?'backward':'forward';
      if(this.node.selectionStart!==start||this.node.selectionEnd!==end||(start!==end&&this.node.selectionDirection!==direction))
        this.node.setSelectionRange(start,end,direction);
    }
    snapshot() {return Object.freeze({value:this.state.value,selection:this.state.selection,revision:this.token});}
    matches(revision) {return !this.disposed&&revision===this.token;}
    lastError() {return this.error;}
    check(revision) {
      if(this.disposed)throw new TextEditorError('Disposed');
      if(this.busy||this.composition||this.pendingHistory)throw new TextEditorError('Busy');
      if(revision!==this.token)throw new TextEditorError('StaleRevision');
      if(this.node.ownerDocument!==this.document||!this.node.isConnected||this.view.closed)throw new TextEditorError('Unavailable');
    }
    editable() {
      return this.node.isConnected && this.node.ownerDocument===this.document && !this.node.matches(':disabled') &&
        !this.node.readOnly && !this.node.closest('[hidden],[inert],[aria-disabled="true"]') &&
        this.view.getComputedStyle(this.node).visibility==='visible' && this.node.getClientRects().length!==0;
    }
    publish(value,selection,kind,{notify=true,history=false,reset=false}={}) {
      const analysis=this.analyze(value);selection=this.validateSelection(selection,analysis);
      const same=value===this.state.value&&selection.anchor===this.state.selection.anchor&&selection.focus===this.state.selection.focus;
      if(same){this.apply(analysis);return false;}
      if(this.token.sequence>=this.maximumRevision)throw new TextEditorError('RevisionExhausted');
      this.busy=true;
      try {
        if(history&&value!==this.state.value) {
          this.redoStates=[];
          if(this.historyLimit){if(this.undoStates.length===this.historyLimit)this.undoStates.shift();this.undoStates.push(this.state);}
        } else if(reset&&value!==this.state.value){this.undoStates=[];this.redoStates=[];}
        this.state=Object.freeze({value,selection:Object.freeze(selection)});
        this.token=Object.freeze({sequence:this.token.sequence+1});this.error=null;this.apply(analysis);
        if(notify&&this.onEdit){try{this.onEdit(this.snapshot(),kind);}catch(error){if(this.nativeDispatch)this.callbackFailed=true;throw error;}}
        return true;
      } finally {this.busy=false;}
    }
    update(revision,{value,selection=this.state.selection}) {
      this.check(revision);return this.publish(value,selection,'update',{notify:false,reset:true});
    }
    setSelection(revision,selection) {
      this.check(revision);return this.publish(this.state.value,selection,'selection',{notify:false});
    }
    replace(revision,range,replacement) {
      this.check(revision);const analysis=this.analyze(this.state.value),replacementAnalysis=this.analyze(replacement);
      const start=analysis.boundaries.find(b=>b.byte===range?.start),end=analysis.boundaries.find(b=>b.byte===range?.end);
      if(!start||!end||start.byte>end.byte)throw new TextEditorError('InvalidInput');
      const value=this.state.value.slice(0,start.utf16)+replacement+this.state.value.slice(end.utf16);
      if(value===this.state.value){this.apply(analysis);return false;}
      if(analysis.bytes-(end.byte-start.byte)+replacementAnalysis.bytes>this.maximumBytes)throw new TextEditorError('Capacity');
      const next=this.analyze(value),insertion=start.utf16+replacement.length;
      const caret=next.boundaries.find(b=>b.utf16>=insertion).byte;
      return this.publish(value,{anchor:caret,focus:caret},'edit',{history:true});
    }
    undo(revision) {return this.history(revision,'undo');}
    redo(revision) {return this.history(revision,'redo');}
    history(revision,kind) {
      this.check(revision);const source=kind==='undo'?this.undoStates:this.redoStates,destination=kind==='undo'?this.redoStates:this.undoStates;
      if(!source.length){this.apply();return false;}
      if(this.token.sequence>=this.maximumRevision)throw new TextEditorError('RevisionExhausted');
      const next=source.at(-1),prior=this.state;
      // Mutate history only after the complete candidate/revision has validated.
      const analysis=this.analyze(next.value);this.validateSelection(next.selection,analysis);
      source.pop();destination.push(prior);return this.publish(next.value,next.selection,kind);
    }
    native(work) {
      if(this.disposed||this.busy)return;
      this.nativeDispatch=true;this.callbackFailed=false;
      try {work();}catch(error){
        this.error=this.callbackFailed?'Callback':(error instanceof TextEditorError?error.code:'InvalidInput');
        if(!this.disposed&&!this.callbackFailed)this.apply();
      }finally{this.nativeDispatch=false;this.callbackFailed=false;}
    }
    selectionOnly() {
      if(this.composition||this.pendingHistory||!this.editable()||this.node.value!==this.state.value)return;
      const analysis=this.analyze(this.state.value);this.publish(this.state.value,this.nativeSelection(analysis),'selection');
    }
    beforeInput(event) {
      if(event.defaultPrevented||this.composition||event.isComposing)return;
      if(this.pendingHistory){if(event.cancelable)event.preventDefault();throw new TextEditorError('Busy');}
      if(!this.editable()){if(event.cancelable)event.preventDefault();this.apply();return;}
      const type=event.inputType;
      if(type==='historyUndo'||type==='historyRedo') {
        const kind=type==='historyUndo'?'undo':'redo';
        if(event.cancelable){event.preventDefault();this.history(this.token,kind);}else{this.pendingHistory=kind;}
        return;
      }
      if(!event.cancelable)return;
      const inserts=type==='insertText'||type==='insertLineBreak'||type==='insertParagraph';
      const deletes=type==='deleteContentBackward'||type==='deleteContentForward';
      if(!inserts&&!deletes)return;
      event.preventDefault();this.selectionOnly();if(this.disposed)return;
      const analysis=this.analyze(this.state.value),selection=this.state.selection;
      let start=Math.min(selection.anchor,selection.focus),end=Math.max(selection.anchor,selection.focus);
      let replacement='';
      if(inserts){replacement=type==='insertText'?event.data:'\n';if(typeof replacement!=='string')throw new TextEditorError('InvalidInput');}
      else if(start===end){const index=analysis.boundaries.findIndex(b=>b.byte===start);if(type==='deleteContentBackward')start=analysis.boundaries[Math.max(0,index-1)].byte;else end=analysis.boundaries[Math.min(analysis.boundaries.length-1,index+1)].byte;}
      const changed=this.replace(this.token,{start,end},replacement);
      if(!changed&&inserts){const insertion=analysis.boundaries.find(b=>b.byte===start).utf16+replacement.length;
        const caret=analysis.boundaries.find(b=>b.utf16>=insertion)?.byte;
        if(caret!==undefined)this.publish(this.state.value,{anchor:caret,focus:caret},'selection');}
    }
    input() {
      if(this.composition)return;
      if(this.pendingHistory){const kind=this.pendingHistory;this.pendingHistory=null;this.history(this.token,kind);return;}
      this.reconcileInput();
    }
    reconcileInput() {
      if(!this.editable()){this.apply();return;}
      const value=this.node.value,analysis=this.analyze(value),selection=this.nativeSelection(analysis);
      if(value===this.state.value){this.publish(value,selection,'selection');return;}
      this.publish(value,selection,'edit',{history:true});
    }
    keydown(event) {
      if(event.defaultPrevented||event.isComposing||this.composition||event.altKey||(!event.ctrlKey&&!event.metaKey))return;
      const key=event.key.toLowerCase();
      const kind=key==='z'?(event.shiftKey?'redo':'undo'):(key==='y'&&event.ctrlKey&&!event.shiftKey?'redo':null);
      if(!kind||!this.editable())return;event.preventDefault();this.history(this.token,kind);
    }
    cancelPending() {
      if(this.compositionTimer!==null){this.view.clearTimeout(this.compositionTimer);this.compositionTimer=null;}
      const pending=this.composition||this.pendingHistory;this.composition=false;this.pendingHistory=null;
      if(pending&&!this.disposed)this.apply();
    }
    dispose() {
      if(this.disposed)return;
      this.cancelPending();this.disposed=true;this.observer?.disconnect();
      for(const [target,type,handler] of this.listeners)target.removeEventListener(type,handler);this.listeners=[];
      if(this.originalMaximum===null)this.node.removeAttribute('maxlength');else this.node.setAttribute('maxlength',this.originalMaximum);
    }
  }
  // Consumer-owned markup/placement; one-level action menu and exact DOM ownership.
  class PopupMenu {
    constructor(trigger,root,entries,{label,onAction}={}) {
      const doc=root?.ownerDocument,view=doc?.defaultView;
      if(!view||view.closed||!(trigger instanceof view.HTMLButtonElement)||trigger.type!=='button'||!trigger.isConnected||
          trigger.ownerDocument!==doc||root?.nodeType!==1||!root.isConnected||root.contains(trigger)||trigger.contains(root)||
          !Array.isArray(entries)||entries.length<1||entries.length>256||
          typeof label!=='string'||label.trim()===''||label.length>256||typeof onAction!=='function'||
          new Set(entries.map(entry=>entry?.node)).size!==entries.length||
          new Set(entries.map(entry=>entry?.action)).size!==entries.length||
          entries.some(entry=>!(entry?.node instanceof view.HTMLButtonElement)||entry.node.type!=='button'||entry.node===root||
            !root.contains(entry.node)||entry.node.ownerDocument!==doc||typeof entry.action!=='string'||entry.action.trim()===''||entry.action.length>128)||
          entries.some((entry,i)=>entries.some((other,j)=>i!==j&&entry.node.contains(other.node))))
        throw new TypeError('Invalid one-level popup menu');
      this.trigger=trigger;this.root=root;this.entries=entries.map(({node,action})=>Object.freeze({node,action}));
      this.document=doc;this.view=view;this.onAction=onAction;this.listeners=[];this.opened=false;this.disposed=false;this.busy=false;this.error=null;this.triggerPointerOpen=false;this.pointerTimer=null;this.pointer=null;
      this.attributes=[];
      const own=(node,name,value)=>{this.attributes.push([node,name,node.getAttribute(name)]);if(value===null)node.removeAttribute(name);else node.setAttribute(name,value);};
      own(trigger,'aria-haspopup','menu');own(trigger,'aria-expanded','false');own(root,'role','menu');own(root,'aria-label',label);own(root,'aria-labelledby',null);own(root,'hidden','');
      for(const {node} of this.entries)own(node,'role','menuitem');
      this.focusGroup=new RovingFocus(root,this.entries.map(entry=>entry.node),{orientation:'vertical',wrap:true});
      const listen=(node,type,handler,capture=false)=>{node.addEventListener(type,handler,capture);this.listeners.push([node,type,handler,capture]);};
      listen(trigger,'pointerdown',event=>{if(event.button!==0||this.pointer!==null)return;this.pointer=event.pointerId;this.triggerPointerOpen=this.opened;});
      listen(view,'pointercancel',event=>{if(event.pointerId===this.pointer)this.clearPointerIntent();});
      listen(view,'pointerup',event=>{if(event.pointerId!==this.pointer)return;if(this.pointerTimer!==null)view.clearTimeout(this.pointerTimer);this.pointerTimer=view.setTimeout(()=>this.clearPointerIntent(),0);});
      listen(trigger,'click',()=>{const close=this.opened||this.triggerPointerOpen;this.clearPointerIntent();if(close)this.close();else this.open();});
      listen(trigger,'keydown',event=>{
        if(event.defaultPrevented||event.altKey||event.ctrlKey||event.metaKey||event.shiftKey||!['ArrowDown','ArrowUp'].includes(event.key))return;
        event.preventDefault();this.open(event.key==='ArrowUp'?'last':'first');
      });
      listen(root,'keydown',event=>{if(this.opened&&!event.defaultPrevented&&event.key==='Escape'&&!event.altKey&&!event.ctrlKey&&!event.metaKey&&!event.shiftKey){event.preventDefault();this.close();}});
      for(const entry of this.entries)listen(entry.node,'click',event=>{if(event.defaultPrevented)return;this.activate(entry);});
      listen(doc,'focusin',event=>{if(this.opened&&!root.contains(event.target))this.close(false);},true);
      listen(doc,'pointerdown',event=>{if(this.opened&&!root.contains(event.target)&&!trigger.contains(event.target))this.close(root.contains(doc.activeElement));},true);
      listen(view,'blur',()=>{this.clearPointerIntent();this.close(false);});listen(doc,'visibilitychange',()=>{if(doc.hidden)this.close(false);});
      listen(doc,'keyup',event=>{if(event.key==='Tab'&&this.opened)this.refresh();});
      this.observer=new view.MutationObserver(()=>this.refresh());
      this.observer.observe(doc,{subtree:true,childList:true,attributes:true,attributeFilter:['disabled','hidden','inert','aria-disabled','style','class']});
    }
    usable(node) {
      return !this.view.closed&&node.isConnected&&node.ownerDocument===this.document&&!node.matches(':disabled')&&
        !node.closest('[hidden],[inert],[aria-disabled="true"]')&&this.view.getComputedStyle(node).visibility==='visible'&&node.getClientRects().length!==0;
    }
    open(edge='first') {
      if(this.disposed||this.busy||!['first','last'].includes(edge)||!this.usable(this.trigger)||!this.root.isConnected||this.root.ownerDocument!==this.document)return false;
      this.root.hidden=false;
      const indices=this.focusGroup.refresh();
      if(!indices.length){this.root.hidden=true;return false;}
      this.opened=true;this.trigger.setAttribute('aria-expanded','true');
      const index=edge==='last'?indices.at(-1):indices[0],focused=this.focusGroup.focus(index);
      if(this.disposed||!this.opened||!focused||this.root.hidden||!this.root.isConnected||
          this.root.ownerDocument!==this.document||this.trigger.ownerDocument!==this.document||
          !this.usable(this.trigger)||this.document.activeElement!==this.entries[index].node){
        if(!this.disposed)this.close(false);return false;
      }
      return true;
    }
    close(restore=true) {
      if(this.disposed||!this.opened)return false;
      const priorBusy=this.busy;this.busy=true;
      try{
        const owned=this.root.contains(this.document.activeElement);
        this.opened=false;this.root.hidden=true;this.trigger.setAttribute('aria-expanded','false');this.focusGroup.refresh();
        if(restore&&owned&&this.usable(this.trigger))this.trigger.focus();return true;
      }finally{this.busy=priorBusy;}
    }
    refresh() {
      if(this.disposed)return [];
      if(!this.root.isConnected||!this.trigger.isConnected||this.root.ownerDocument!==this.document||this.trigger.ownerDocument!==this.document){this.dispose();return [];}
      if(!this.opened)return [];
      if(!this.root.contains(this.document.activeElement)){this.close(false);return [];}
      if(!this.usable(this.trigger)||this.root.hidden){this.close(false);return [];}
      const indices=this.focusGroup.refresh();if(!indices.length)this.close();return indices;
    }
    activate(entry) {
      if(this.disposed||this.busy||!this.opened||!this.entries.includes(entry)||!this.usable(entry.node))return false;
      this.busy=true;
      try{this.close();if(this.disposed)return false;this.onAction(entry.action,entry.node);this.error=null;return true;}
      catch{this.error='Callback';return false;}
      finally{this.busy=false;}
    }
    lastError(){return this.error;}
    clearPointerIntent(){if(this.pointerTimer!==null){this.view.clearTimeout(this.pointerTimer);this.pointerTimer=null;}this.triggerPointerOpen=false;this.pointer=null;}
    dispose() {
      if(this.disposed)return;
      this.clearPointerIntent();this.close(false);this.disposed=true;this.observer.disconnect();
      for(const [node,type,handler,capture] of this.listeners)node.removeEventListener(type,handler,capture);this.listeners=[];
      this.focusGroup.dispose();
      for(const [node,name,value] of this.attributes){if(value===null)node.removeAttribute(name);else node.setAttribute(name,value);}this.attributes=[];
    }
  }
  class TabbedPanels {
    constructor(root,entries,{label,activation='manual',orientation='horizontal',rtl=false,wrap=true,index=0,onActivate=null}={}) {
      const doc=root?.ownerDocument,view=doc?.defaultView;
      const nodes=Array.isArray(entries)?entries.flatMap(entry=>[entry?.tab,entry?.panel]):[];
      if(!view||view.closed||root?.nodeType!==1||!root.isConnected||!Array.isArray(entries)||entries.length<1||entries.length>256||
          typeof label!=='string'||label.trim()===''||label.length>256||!['manual','automatic'].includes(activation)||
          !['horizontal','vertical'].includes(orientation)||typeof rtl!=='boolean'||typeof wrap!=='boolean'||
          !Number.isInteger(index)||index<0||index>=entries.length||(onActivate!==null&&typeof onActivate!=='function')||
          new Set(nodes).size!==nodes.length||entries.some(entry=>!(entry?.tab instanceof view.HTMLButtonElement)||entry.tab.type!=='button'||
            entry.tab===root||!root.contains(entry.tab)||entry.panel?.nodeType!==1||typeof entry.panel.focus!=='function'||
            !entry.panel.isConnected||entry.panel.ownerDocument!==doc||root.contains(entry.panel)||entry.panel.contains(root))||
          nodes.some(node=>!node.isConnected||node.ownerDocument!==doc||!node.id||node.id.length>128||/\s/.test(node.id)||doc.getElementById(node.id)!==node)||
          new Set(nodes.map(node=>node.id)).size!==nodes.length||nodes.some((node,i)=>nodes.some((other,j)=>i!==j&&node.contains(other))))
        throw new TypeError('Invalid tabbed panels');
      this.root=root;this.entries=entries.map(({tab,panel})=>Object.freeze({tab,panel,tabId:tab.id,panelId:panel.id}));
      this.document=doc;this.view=view;this.activation=activation;this.onActivate=onActivate;this.disposed=false;this.busy=false;this.error=null;this.index=null;this.listeners=[];this.attributes=[];
      const own=(node,name,value)=>{this.attributes.push([node,name,node.getAttribute(name)]);this.write(node,name,value);};
      own(root,'role','tablist');own(root,'aria-label',label);own(root,'aria-labelledby',null);own(root,'aria-orientation',orientation);
      for(const {tab,panel,panelId,tabId} of this.entries){own(tab,'role','tab');own(tab,'aria-selected','false');own(tab,'aria-controls',panelId);
        own(panel,'role','tabpanel');own(panel,'aria-labelledby',tabId);own(panel,'hidden','');own(panel,'tabindex','0');}
      this.focusGroup=new RovingFocus(root,this.entries.map(entry=>entry.tab),{orientation,rtl,wrap,index,onFocus:node=>{
        if(!this.disposed&&!this.busy&&this.activation==='automatic')this.change(this.entries.findIndex(entry=>entry.tab===node),false,true,'focus');
      }});
      const eligible=this.focusGroup.refresh();this.render(eligible.includes(index)?index:eligible[0]??null);
      if(this.index!==null){this.focusGroup.index=this.index;this.focusGroup.refresh();}
      const listen=(node,type,handler)=>{node.addEventListener(type,handler);this.listeners.push([node,type,handler]);};
      for(let i=0;i<this.entries.length;i++)listen(this.entries[i].tab,'click',event=>{if(!event.defaultPrevented)this.change(i,true,true,'activate');});
      this.observer=new view.MutationObserver(()=>this.refresh());
      this.observer.observe(doc,{subtree:true,childList:true,attributes:true,attributeFilter:['disabled','hidden','inert','aria-disabled','style','class','id','type']});
    }
    write(node,name,value){if(value===null){if(node.hasAttribute(name))node.removeAttribute(name);}else if(node.getAttribute(name)!==value)node.setAttribute(name,value);}
    valid(){return !this.view.closed&&this.root.isConnected&&this.root.ownerDocument===this.document&&this.entries.every(({tab,panel,tabId,panelId})=>
      tab.isConnected&&panel.isConnected&&tab.type==='button'&&tab.ownerDocument===this.document&&panel.ownerDocument===this.document&&this.root.contains(tab)&&!this.root.contains(panel)&&!panel.contains(this.root)&&
      tab.id===tabId&&panel.id===panelId&&this.document.getElementById(tabId)===tab&&this.document.getElementById(panelId)===panel)&&this.entries.every((entry,i)=>this.entries.every((other,j)=>i===j||(!entry.tab.contains(other.tab)&&!entry.panel.contains(other.panel)&&!entry.panel.contains(other.tab))));}
    settled(index,focus=false){
      if(this.disposed)return false;
      if(!this.valid()){this.dispose();return false;}
      return this.index===index&&(index===null?!this.entries.some(entry=>this.focusGroup.eligible(entry.tab)):this.focusGroup.eligible(this.entries[index].tab))&&
        (!focus||(index!==null&&this.document.activeElement===this.entries[index].tab));
    }
    render(index){this.index=index;for(let i=0;i<this.entries.length;i++){const {tab,panel}=this.entries[i];this.write(tab,'aria-selected',i===index?'true':'false');this.write(panel,'hidden',i===index?null:'');}}
    change(index,focus,notify,reason){
      if(this.disposed||this.busy||!Number.isInteger(index)||index<0||index>=this.entries.length||typeof focus!=='boolean'||typeof notify!=='boolean')return false;
      if(!this.valid()){this.dispose();return false;}if(!this.focusGroup.eligible(this.entries[index].tab))return false;
      this.busy=true;
      try{
        const changed=this.index!==index;this.render(index);
        if(focus&&!this.focusGroup.focus(index))return false;
        if(!this.settled(index,focus))return false;
        if(changed&&notify&&this.onActivate)this.onActivate(Object.freeze({index,tab:this.entries[index].tab,panel:this.entries[index].panel,reason}));
        if(!this.settled(index,focus))return false;this.error=null;return true;
      }catch{this.error='Callback';if(!this.disposed&&!this.valid())this.dispose();return false;}finally{this.busy=false;}
    }
    activate(index,{focus=true,notify=true}={}){return this.change(index,focus,notify,'activate');}
    selected(){return this.disposed?null:this.index;}
    lastError(){return this.error;}
    refresh(){
      if(this.disposed||this.busy)return [];
      if(!this.valid()){this.dispose();return [];}
      this.busy=true;
      try{
        const eligible=this.focusGroup.refresh();if(eligible.includes(this.index)){this.render(this.index);return eligible;}
        const prior=this.index,next=eligible[0]??null,owned=prior!==null&&this.entries[prior].panel.contains(this.document.activeElement);
        this.render(next);
        const requiredFocus=owned&&next!==null;
        if(requiredFocus&&!this.focusGroup.focus(next))return [];
        if(!this.settled(next,requiredFocus))return [];
        if(prior!==next&&this.onActivate)this.onActivate(Object.freeze({index:next,tab:next===null?null:this.entries[next].tab,panel:next===null?null:this.entries[next].panel,reason:'eligibility'}));
        if(!this.settled(next,requiredFocus))return [];
        this.error=null;return this.focusGroup.refresh();
      }catch{this.error='Callback';if(!this.disposed&&!this.valid())this.dispose();return [];}finally{this.busy=false;}
    }
    dispose(){
      if(this.disposed)return;this.disposed=true;this.observer?.disconnect();
      for(const [node,type,handler] of this.listeners)node.removeEventListener(type,handler);this.listeners=[];
      this.focusGroup?.dispose();for(const [node,name,value] of this.attributes)this.write(node,name,value);this.attributes=[];
    }
  }
  const modalOwners=new WeakMap(),dialogOwners=new WeakMap();
  class ModalDialog {
    constructor(node,{label,initialFocus,onClose=null}={}) {
      const doc=node?.ownerDocument,view=doc?.defaultView;
      if(!view||view.closed||typeof view.HTMLDialogElement!=='function'||!(node instanceof view.HTMLDialogElement)||
          !node.isConnected||node.open||typeof node.showModal!=='function'||typeof node.close!=='function'||
          typeof label!=='string'||label.trim()===''||label.length>256||initialFocus?.nodeType!==1||
          initialFocus===node||!node.contains(initialFocus)||!initialFocus.isConnected||initialFocus.ownerDocument!==doc||
          typeof initialFocus.focus!=='function'||(onClose!==null&&typeof onClose!=='function'))throw new TypeError('Invalid native modal dialog');
      try{node.matches(':modal');}catch{throw new TypeError('Native modal selector unavailable');}
      if(node.matches(':modal')||dialogOwners.has(node))throw new TypeError('Dialog already owned or modal');
      this.node=node;this.initialFocus=initialFocus;this.document=doc;this.view=view;this.onClose=onClose;
      this.state='idle';this.disposed=false;this.busy=false;this.error=null;this.generation=null;this.pending=null;this.openingCall=false;this.completing=false;this.drain=null;dialogOwners.set(node,this);
      this.originalValue=node.returnValue;this.attributes=['aria-label','aria-labelledby','aria-modal'].map(name=>[name,node.getAttribute(name)]);
      node.setAttribute('aria-label',label);node.removeAttribute('aria-labelledby');node.setAttribute('aria-modal','true');
      this.cancel=event=>{if(!event.defaultPrevented&&this.state==='open'&&!this.busy&&!this.disposed){event.preventDefault();this.requestClose('','cancel');}};
      this.closed=event=>{if(event.isTrusted)this.complete();};node.addEventListener('cancel',this.cancel);node.addEventListener('close',this.closed);
      this.observer=new view.MutationObserver(()=>this.refresh());
      this.observer.observe(doc,{subtree:true,childList:true,attributes:true,attributeOldValue:true,attributeFilter:['open','hidden','inert','disabled','aria-disabled','class','style']});
    }
    valid(){return !this.view.closed&&this.node.isConnected&&this.initialFocus.isConnected&&
      this.node.ownerDocument===this.document&&this.initialFocus.ownerDocument===this.document&&this.node.contains(this.initialFocus);}
    usable(node){return node.isConnected&&node.ownerDocument===this.document&&!node.matches(':disabled')&&
      !node.closest('[hidden],[inert],[aria-disabled="true"]')&&this.view.getComputedStyle(node).visibility==='visible'&&node.getClientRects().length>0;}
    valueValid(value){return typeof value==='string'&&value.length<=128&&!/[\uD800-\uDFFF]/u.test(value);}
    stopNative(){
      // Consumer open-attribute removal strands native modality in both tested engines.
      // Repair using native methods; never assign an artificial open attribute.
      if(!this.node.open&&this.node.matches(':modal')&&this.node.isConnected)this.node.showModal();
      if(this.node.open)this.node.close();
    }
    open(){
      if(this.disposed||this.busy||this.state!=='idle'||modalOwners.has(this.document))return false;
      if(!this.valid()){this.dispose();return false;}
      if(this.node.open||this.node.matches(':modal')){this.error='Ownership';this.dispose();return false;}
      if(this.document.querySelector('dialog:modal'))return false;
      this.busy=true;this.openingCall=true;this.state='opening';this.openedNative=false;this.generation=Object.freeze({});this.pending=null;modalOwners.set(this.document,this);
      try{
        this.node.returnValue='';this.node.showModal();
        this.openedNative=this.node.open||this.node.matches(':modal')||this.observer.takeRecords().some(record=>record.target===this.node&&record.attributeName==='open');
        if(this.disposed){this.closeDisposedNative();return false;}
        if(!this.valid()||!this.node.open||!this.node.matches(':modal')||!this.usable(this.node)||!this.usable(this.initialFocus)){this.abandon();return false;}
        this.initialFocus.focus({preventScroll:true});
        if(this.disposed){this.closeDisposedNative();return false;}
        if(!this.valid()||!this.node.open||!this.node.matches(':modal')||!this.usable(this.initialFocus)||this.document.activeElement!==this.initialFocus){this.abandon();return false;}
        this.state='open';this.error=null;return true;
      }catch{this.error='Native';if(!this.disposed)this.abandon();return false;}
      finally{this.openingCall=false;if(this.disposed){if(this.drain&&!this.drain.awaiting)this.releaseDrain();this.releaseDocument();}this.busy=false;}
    }
    abandon(){
      if(this.openedNative||this.node.open||this.node.matches(':modal')){
        this.state='closing';this.pending={generation:this.generation,quiet:true,reason:'close',value:''};this.stopNative();
      }else{this.state='idle';this.generation=null;if(modalOwners.get(this.document)===this)modalOwners.delete(this.document);}
    }
    requestClose(value,reason){
      if(this.disposed||this.busy||this.state!=='open'||!this.valueValid(value))return false;
      if(!this.valid()){this.dispose();return false;}
      if(!this.node.open||!this.node.matches(':modal')){this.refresh();return false;}
      this.busy=true;this.state='closing';this.pending={generation:this.generation,reason,value,quiet:false};
      try{this.node.close(value);return !this.disposed&&!this.node.open&&!this.node.matches(':modal');}
      catch{this.error='Native';this.state='open';this.pending=null;return false;}
      finally{this.busy=false;}
    }
    close(value=''){return this.requestClose(value,'close');}
    complete(){
      if(this.disposed||this.busy||!['open','closing'].includes(this.state))return;
      if(this.node.open||this.node.matches(':modal')){this.error='Ownership';this.dispose();return;}
      this.busy=true;this.completing=true;
      const pending=this.pending??{generation:this.generation,reason:'native',value:this.node.returnValue,quiet:false};
      try{
        if(!this.valid()){this.dispose();return;}
        if(pending.generation!==this.generation)return;
        if(pending.reason==='native')pending.value=this.node.returnValue;
        if(!this.valueValid(pending.value)){this.error='Value';return;}
        if(!pending.quiet&&this.onClose)this.onClose(Object.freeze({value:pending.value,reason:pending.reason}));
        if(!this.disposed&&(!this.valid()||this.node.open||this.node.matches(':modal'))){this.error='Ownership';this.dispose();}
        if(!this.disposed&&!pending.quiet)this.error=null;
      }catch{this.error='Callback';if(!this.disposed&&!this.valid())this.dispose();}
      finally{
        if(!this.disposed)this.state='idle';this.pending=null;this.generation=null;
        if(modalOwners.get(this.document)===this)modalOwners.delete(this.document);if(this.disposed)this.releaseDocument();this.completing=false;this.busy=false;
      }
    }
    refresh(){
      if(this.disposed||this.busy)return false;
      if(!this.valid()){this.dispose();return false;}
      if(this.state==='idle'){
        if(this.node.open||this.node.matches(':modal')){this.error='Ownership';this.dispose();return false;}return true;
      }
      if(!this.node.open){
        if(this.node.matches(':modal')){this.error='Ownership';this.dispose();return false;}
        // A real native close queues its close event; retain this lifecycle's owner.
        if(this.state==='open'){this.state='closing';this.pending={generation:this.generation,reason:'native',value:this.node.returnValue,quiet:false};}
        return true;
      }
      if(this.state==='closing'||!this.node.matches(':modal')){this.error='Ownership';this.dispose();return false;}
      if(!this.usable(this.node)||!this.usable(this.initialFocus)){this.dispose();return false;}return true;
    }
    isOpen(){return !this.disposed&&this.state==='open'&&this.node.open&&this.node.matches(':modal');}
    lastError(){return this.error;}
    isDraining(){return this.drain!==null;}
    ensureDrain(awaiting){
      if(this.drain){this.drain.awaiting ||= awaiting;return;}
      const drain={awaiting,node:this.node,document:this.document,handler:null};this.drain=drain;
      dialogOwners.set(this.node,drain);
      if(!modalOwners.has(this.document)||modalOwners.get(this.document)===this)modalOwners.set(this.document,drain);
      drain.handler=event=>{
        if(!event.isTrusted)return;
        if(this.node.open||this.node.matches(':modal')){try{this.stopNative();}catch{this.error='Native';}return;}
        this.releaseDrain();
      };
      this.node.addEventListener('close',drain.handler);
    }
    releaseDocument(){
      if(this.node.open||this.node.matches(':modal'))return;
      const owner=modalOwners.get(this.document);
      if(owner===this||(this.drain&&owner===this.drain))modalOwners.delete(this.document);
    }
    releaseDrain(){
      const drain=this.drain;if(!drain)return;this.node.removeEventListener('close',drain.handler);
      if(dialogOwners.get(this.node)===drain)dialogOwners.delete(this.node);
      if(modalOwners.get(this.document)===drain)modalOwners.delete(this.document);this.drain=null;
    }
    closeDisposedNative(){
      if(this.node.open||this.node.matches(':modal')){this.ensureDrain(true);this.stopNative();}
    }
    dispose(){
      if(this.disposed)return;
      const state=this.state,hadOpenMutation=this.observer.takeRecords().some(record=>record.target===this.node&&record.attributeName==='open');
      const awaiting=this.node.open||this.node.matches(':modal')||(!this.completing&&(state==='closing'||
        (this.openingCall&&(this.openedNative||hadOpenMutation))));
      if(awaiting||this.openingCall)this.ensureDrain(awaiting);
      this.disposed=true;this.onClose=null;this.state='disposed';this.observer.disconnect();
      this.node.removeEventListener('cancel',this.cancel);this.node.removeEventListener('close',this.closed);
      const priorBusy=this.busy;this.busy=true;
      try{this.stopNative();}catch{this.error='Native';}
      finally{
        if(!this.drain){if(dialogOwners.get(this.node)===this)dialogOwners.delete(this.node);
          if(!this.completing&&modalOwners.get(this.document)===this)modalOwners.delete(this.document);}
        this.pending=null;this.generation=null;
        for(const [name,value] of this.attributes){if(value===null)this.node.removeAttribute(name);else this.node.setAttribute(name,value);}this.attributes=[];
        this.node.returnValue=this.originalValue;if(!this.openingCall&&!this.completing)this.releaseDocument();this.busy=priorBusy;
      }
    }
  }
  const splitRoots=new WeakMap(),splitSeparators=new WeakMap();
  class SplitPane {
    constructor(root,first,separator,second,{label,axis='horizontal',rtl=false,value=.5,minimumPixels=0,secondMinimumPixels=0,thickness=8,step=.01,onResize=null}={}){
      const doc=root?.ownerDocument,view=doc?.defaultView,nodes=[root,first,separator,second],idValid=node=>typeof node?.id==='string'&&node.id.length>0&&node.id.length<=128&&!/\s/u.test(node.id)&&doc.getElementById(node.id)===node;
      if(!view||view.closed||typeof view.ResizeObserver!=='function'||nodes.some(node=>!(node instanceof view.HTMLElement)||!node.isConnected||node.ownerDocument!==doc)||
         new Set(nodes).size!==4||!SplitPane.directContent(root)||root.children.length!==3||root.children[0]!==first||root.children[1]!==separator||root.children[2]!==second||
         separator.children.length!==0||separator.isContentEditable||separator.matches('button,input,select,textarea,a[href],area[href],summary,iframe,object,embed,audio[controls],video[controls],[contenteditable]:not([contenteditable="false"])')||
         !idValid(first)||!idValid(second)||first.id===second.id||typeof label!=='string'||label.trim()===''||label.length>256||
         !['horizontal','vertical'].includes(axis)||typeof rtl!=='boolean'||!Number.isFinite(value)||value<0||value>1||
         !Number.isInteger(minimumPixels)||minimumPixels<0||minimumPixels>4096||!Number.isInteger(secondMinimumPixels)||secondMinimumPixels<0||secondMinimumPixels>4096||
         !Number.isInteger(thickness)||thickness<1||thickness>64||!Number.isFinite(step)||step<=0||step>1||(onResize!==null&&typeof onResize!=='function')||
         splitRoots.has(root)||splitSeparators.has(separator))throw new TypeError('Invalid local split pane');
      Object.assign(this,{root,first,separator,second,document:doc,view,axis,rtl,minimumPixels,secondMinimumPixels,thickness,step,onResize,value,firstId:first.id,secondId:second.id});
      this.disposed=false;this.busy=false;this.pointer=null;this.error=null;this.status='initial';this.listeners=[];this.attributes=[];this.styles=[];this.styleWrites=new Map();this.originalStyles=[];
      for(const node of nodes){const raw=node.getAttribute('style');if(raw!==null&&raw.length>16384)throw new TypeError('Split pane original style capacity');this.originalStyles.push([node,raw,node.style.cssText]);}
      const rootStyles=['display','grid-template-columns','grid-template-rows','grid-template-areas','grid-auto-flow','grid-auto-columns','grid-auto-rows','row-gap','column-gap','writing-mode','direction','overflow-x','overflow-y'];
      const placementStyles=['position','float','order','grid-row-start','grid-row-end','grid-column-start','grid-column-end','align-self','justify-self'];
      const panelStyles=[...placementStyles,'width','height','min-width','min-height','max-width','max-height','box-sizing','overflow-x','overflow-y',...['margin','padding','border'].flatMap(prefix=>['top','right','bottom','left'].map(side=>prefix==='border'?`border-${side}-width`:`${prefix}-${side}`))];
      const separatorStyles=[...placementStyles,'width','height','min-width','min-height','max-width','max-height','box-sizing','touch-action',...['margin','padding','border'].flatMap(prefix=>['top','right','bottom','left'].map(side=>prefix==='border'?`border-${side}-width`:`${prefix}-${side}`))];
      for(const [node,names] of [[root,rootStyles],[first,panelStyles],[second,panelStyles],[separator,separatorStyles]])for(const name of names){const original=node.style.getPropertyValue(name);if(original.length>4096)throw new TypeError('Split pane original style capacity');this.styles.push([node,name,original,node.style.getPropertyPriority(name)]);}
      for(const name of ['role','aria-label','aria-labelledby','aria-orientation','aria-controls','aria-valuemin','aria-valuemax','aria-valuenow','aria-valuetext','aria-disabled','tabindex']){const original=separator.getAttribute(name);if(original!==null&&original.length>4096)throw new TypeError('Split pane original attribute capacity');this.attributes.push([name,original]);}
      if(!this.pseudoSafe())throw new TypeError('Generated split pane grid item unsupported');
      if(!this.untransformed())throw new TypeError('Transformed split pane unsupported');
      splitRoots.set(root,this);splitSeparators.set(separator,this);
      this.writeStyle(root,'display','grid');this.writeStyle(root,'grid-template-areas','none');this.writeStyle(root,'grid-auto-flow','row');this.writeStyle(root,'grid-auto-columns','auto');this.writeStyle(root,'grid-auto-rows','auto');this.writeStyle(root,'row-gap','0px');this.writeStyle(root,'column-gap','0px');this.writeStyle(root,'writing-mode','horizontal-tb');this.writeStyle(root,'direction',rtl?'rtl':'ltr');this.writeStyle(root,'overflow-x','hidden');this.writeStyle(root,'overflow-y','hidden');
      for(const node of [first,second,separator]){
        this.writeStyle(node,'position','static');this.writeStyle(node,'float','none');this.writeStyle(node,'max-width','none');this.writeStyle(node,'max-height','none');this.writeStyle(node,'order','0');for(const name of ['grid-row-start','grid-row-end','grid-column-start','grid-column-end'])this.writeStyle(node,name,'auto');for(const name of ['align-self','justify-self'])this.writeStyle(node,name,'stretch');
        for(const name of ['width','height'])this.writeStyle(node,name,'auto');for(const name of ['min-width','min-height'])this.writeStyle(node,name,'0px');this.writeStyle(node,'box-sizing','border-box');
        for(const prefix of ['margin','padding','border'])for(const side of ['top','right','bottom','left'])this.writeStyle(node,prefix==='border'?`border-${side}-width`:`${prefix}-${side}`,'0px');
      }
      for(const node of [first,second]){this.writeStyle(node,'max-width','none');this.writeStyle(node,'max-height','none');this.writeStyle(node,'overflow-x','hidden');this.writeStyle(node,'overflow-y','hidden');}
      this.writeStyle(separator,'touch-action','none');this.writeAttr('role','separator');this.writeAttr('aria-label',label);this.writeAttr('aria-labelledby',null);this.writeAttr('aria-controls',`${first.id} ${second.id}`);this.writeAttr('aria-orientation',axis==='horizontal'?'vertical':'horizontal');
      this.listen(separator,'keydown',event=>this.key(event));this.listen(separator,'pointerdown',event=>this.down(event));this.listen(view,'pointermove',event=>this.move(event));
      for(const type of ['pointerup','pointercancel'])this.listen(view,type,event=>{if(this.pointer?.id===event.pointerId)this.stopPointer();});
      this.listen(separator,'lostpointercapture',event=>{if(this.pointer?.id===event.pointerId)this.stopPointer();});this.listen(view,'blur',()=>this.stopPointer());
      this.listen(doc,'visibilitychange',()=>{if(doc.hidden)this.stopPointer();});
      this.mutations=new view.MutationObserver(()=>this.refresh());this.mutations.observe(doc,{subtree:true,childList:true,attributes:true,attributeFilter:['id','hidden','inert','aria-disabled','style','class','contenteditable','href','controls']});
      this.resize=new view.ResizeObserver(()=>{if(!this.disposed)this.refresh();});this.resize.observe(root);this.refresh();
    }
    static directContent(root){if(root.childNodes.length>64)return false;for(const node of root.childNodes){if(node.nodeType===3&&/[^\u0009\u000a\u000c\u000d\u0020]/u.test(node.data))return false;if(![1,3,8].includes(node.nodeType))return false;}return true;}
    pseudoSafe(){for(const pseudo of ['::before','::after']){const style=this.view.getComputedStyle(this.root,pseudo);if(!['none','normal'].includes(style.content)&&style.display!=='none'&&!['absolute','fixed'].includes(style.position))return false;}return true;}
    listen(node,type,handler){node.addEventListener(type,handler);this.listeners.push([node,type,handler]);}
    writeStyle(node,name,value){
      let writes=this.styleWrites.get(node);if(!writes){writes=new Map();this.styleWrites.set(node,writes);}const prior=writes.get(name),current=node.style.getPropertyValue(name);
      if(prior?.input===value&&prior.actual===current&&node.style.getPropertyPriority(name)==='important')return;
      if(current!==value||node.style.getPropertyPriority(name)!=='important')node.style.setProperty(name,value,'important');
      writes.set(name,{input:value,actual:node.style.getPropertyValue(name)});
    }
    writeAttr(name,value){if(this.separator.getAttribute(name)===value)return;if(value===null)this.separator.removeAttribute(name);else this.separator.setAttribute(name,value);}
    valid(){return this.separator.children.length===0&&!this.separator.isContentEditable&&!this.separator.matches('button,input,select,textarea,a[href],area[href],summary,iframe,object,embed,audio[controls],video[controls],[contenteditable]:not([contenteditable="false"])')&&SplitPane.directContent(this.root)&&!this.view.closed&&[this.root,this.first,this.separator,this.second].every(node=>node.isConnected&&node.ownerDocument===this.document)&&
      this.root.children.length===3&&this.root.children[0]===this.first&&this.root.children[1]===this.separator&&this.root.children[2]===this.second&&
      this.first.id===this.firstId&&this.second.id===this.secondId&&this.document.getElementById(this.firstId)===this.first&&this.document.getElementById(this.secondId)===this.second;}
    untransformed(){
      const nodes=new Set([this.first,this.separator,this.second]);let count=0;
      for(let node=this.root;node;node=node.parentElement){if(++count>64)return false;nodes.add(node);}
      for(const node of nodes){const style=this.view.getComputedStyle(node);if(style.transform!=='none'||(style.translate&&style.translate!=='none')||(style.rotate&&style.rotate!=='none')||(style.scale&&style.scale!=='none')||(style.zoom&&style.zoom!=='1'&&style.zoom!=='normal'))return false;}return true;
    }
    available(){
      if(!this.valid()||!this.untransformed()||!this.pseudoSafe())return false;
      for(const node of [this.root,this.first,this.second])if(node.closest('[hidden],[inert],[aria-disabled="true"]')||this.view.getComputedStyle(node).visibility!=='visible')return false;
      return !this.separator.hidden&&!this.separator.closest('[inert]')&&this.view.getComputedStyle(this.separator).visibility==='visible'&&[this.root,this.first,this.separator,this.second].every(node=>node.getClientRects().length>0);
    }
    metrics(){
      const style=this.view.getComputedStyle(this.root),horizontal=this.axis==='horizontal';
      const rect=this.root.getBoundingClientRect();
      const size=(horizontal?rect.width:rect.height)-parseFloat(horizontal?style.borderLeftWidth:style.borderTopWidth)-parseFloat(horizontal?style.borderRightWidth:style.borderBottomWidth)-parseFloat(horizontal?style.paddingLeft:style.paddingTop)-parseFloat(horizontal?style.paddingRight:style.paddingBottom);
      const available=size-this.thickness;
      if(!Number.isFinite(size)||size<=0||available<=0||available<this.minimumPixels+this.secondMinimumPixels)return null;
      return {available,minimum:this.minimumPixels/available,maximum:1-this.secondMinimumPixels/available};
    }
    render(metrics){
      if(metrics)this.value=Math.max(metrics.minimum,Math.min(metrics.maximum,this.value));
      const tracks=`minmax(0,${this.value.toFixed(9)}fr) ${this.thickness}px minmax(0,${(1-this.value).toFixed(9)}fr)`;
      this.writeStyle(this.root,'grid-template-columns',this.axis==='horizontal'?tracks:'minmax(0,1fr)');this.writeStyle(this.root,'grid-template-rows',this.axis==='vertical'?tracks:'minmax(0,1fr)');
      this.writeAttr('aria-valuemin',String((metrics?.minimum??0)*100));this.writeAttr('aria-valuemax',String((metrics?.maximum??1)*100));this.writeAttr('aria-valuenow',String(this.value*100));this.writeAttr('aria-valuetext',`${(this.value*100).toFixed(1)} percent`);
      this.writeAttr('aria-disabled',metrics?'false':'true');this.writeAttr('tabindex',metrics?'0':'-1');
    }
    refresh(){
      if(this.disposed||this.busy)return false;if(!this.valid()){this.error='Ownership';this.dispose();return false;}
      this.busy=true;
      try{
        if(!this.available()){this.stopPointer();this.status='unavailable';this.error=!this.untransformed()?'Transform':!this.pseudoSafe()?'Layout':'Unavailable';this.writeAttr('aria-disabled','true');this.writeAttr('tabindex','-1');return false;}
        const metrics=this.metrics();if(!metrics||(this.pointer&&metrics.available!==this.pointer.available))this.stopPointer();this.render(metrics);this.status=metrics?'ready':'too-small';this.error=metrics?null:'TooSmall';return Boolean(metrics);
      }finally{this.busy=false;}
    }
    currentValue(){return this.value;}
    lastError(){return this.error;}
    setValue(value,{notify=false}={}){
      if(this.disposed||this.busy||this.pointer||!Number.isFinite(value)||value<0||value>1||typeof notify!=='boolean')return false;
      return this.change(value,notify?'programmatic':null);
    }
    change(value,source){
      if(this.disposed||this.busy)return false;if(!this.valid()){this.error='Ownership';this.dispose();return false;}
      if(!this.available()){this.refresh();return false;}const metrics=this.metrics();if(!metrics){this.refresh();return false;}
      value=Math.max(metrics.minimum,Math.min(metrics.maximum,value));if(value===this.value)return true;
      this.busy=true;
      try{this.value=value;this.render(metrics);this.status='ready';this.error=null;if(source&&this.onResize)this.onResize(Object.freeze({value,source}));
        if(this.disposed)return false;if(!this.valid()){this.error='Ownership';this.dispose();return false;}if(!this.available()){this.stopPointer();this.status='unavailable';this.error=!this.untransformed()?'Transform':!this.pseudoSafe()?'Layout':'Unavailable';this.writeAttr('aria-disabled','true');this.writeAttr('tabindex','-1');return false;}
        const finalMetrics=this.metrics();if(!finalMetrics){this.stopPointer();this.render(null);this.status='too-small';this.error='TooSmall';return false;}
        if(this.value<finalMetrics.minimum||this.value>finalMetrics.maximum){this.render(finalMetrics);return false;}return true;
      }catch{this.error='Callback';if(!this.disposed&&!this.valid())this.dispose();return false;}
      finally{this.busy=false;if(this.error==='Callback'&&!this.disposed){this.refresh();this.error='Callback';}}
    }
    key(event){
      if(event.defaultPrevented||event.ctrlKey||event.metaKey||event.altKey||this.disposed||this.busy)return;
      const horizontal=this.axis==='horizontal',negative=horizontal?'ArrowLeft':'ArrowUp',positive=horizontal?'ArrowRight':'ArrowDown';
      if(![negative,positive,'Home','End'].includes(event.key))return;if(!this.refresh())return;const metrics=this.metrics();if(!metrics)return;
      let value=event.key==='Home'?metrics.minimum:event.key==='End'?metrics.maximum:this.value+(event.key===positive?1:-1)*(horizontal&&this.rtl?-1:1)*this.step*(event.shiftKey?.1:1);
      event.preventDefault();this.stopPointer();this.change(value,'keyboard');
    }
    down(event){
      if(event.button!==0||!Number.isSafeInteger(event.pointerId)||event.pointerId<0||!Number.isFinite(this.axis==='horizontal'?event.clientX:event.clientY)||this.pointer||this.disposed||this.busy||!this.refresh())return;
      const first=this.first.getBoundingClientRect(),second=this.second.getBoundingClientRect(),span=this.axis==='horizontal'?first.width+second.width:first.height+second.height;
      if(!Number.isFinite(span)||span<=0)return;this.busy=true;
      try{event.preventDefault();this.pointer={id:event.pointerId,start:this.axis==='horizontal'?event.clientX:event.clientY,value:this.value,span,available:this.metrics()?.available};
        this.separator.focus({preventScroll:true});if(this.disposed)return;if(!this.valid()){this.error='Ownership';this.dispose();return;}
        if(!this.available()||!this.metrics()||this.document.activeElement!==this.separator){this.stopPointer();return;}
        try{this.separator.setPointerCapture(event.pointerId);}catch{}
      }finally{this.busy=false;if(!this.disposed)this.refresh();}
    }
    move(event){
      const pointer=this.pointer;if(this.disposed||this.busy||!pointer||pointer.id!==event.pointerId)return;
      const position=this.axis==='horizontal'?event.clientX:event.clientY;if(!Number.isFinite(position))return;
      const metrics=this.metrics();if(!metrics||metrics.available!==pointer.available){this.stopPointer();this.refresh();return;}
      this.change(pointer.value+(position-pointer.start)/pointer.span*(this.axis==='horizontal'&&this.rtl?-1:1),'pointer');
    }
    stopPointer(){const pointer=this.pointer;this.pointer=null;if(pointer){try{this.separator.releasePointerCapture(pointer.id);}catch{}}}
    dispose(){
      if(this.disposed)return;this.disposed=true;this.onResize=null;this.mutations?.disconnect();this.resize?.disconnect();this.stopPointer();
      for(const [node,type,handler] of this.listeners)node.removeEventListener(type,handler);this.listeners=[];this.styleWrites.clear();
      for(const [name,value] of this.attributes){if(value===null)this.separator.removeAttribute(name);else this.separator.setAttribute(name,value);}this.attributes=[];
      for(const [node,name,value,priority] of this.styles){if(value==='')node.style.removeProperty(name);else node.style.setProperty(name,value,priority);}this.styles=[];
      for(const [node,raw,normalized] of this.originalStyles)if(node.style.cssText===normalized){if(raw===null){node.getAttribute('style');node.removeAttribute('style');}else node.setAttribute('style',raw);}this.originalStyles=[];
      if(splitRoots.get(this.root)===this)splitRoots.delete(this.root);if(splitSeparators.get(this.separator)===this)splitSeparators.delete(this.separator);
    }
  }
  const listRoots=new WeakMap();
  class ListBox {
    constructor(root,options,{label,multiple=false,selected=[],active=0,onSelection=null}={}){
      const doc=root?.ownerDocument,view=doc?.defaultView,interactive='button,input,select,textarea,a[href],area[href],summary,iframe,object,embed,audio[controls],video[controls],[contenteditable]:not([contenteditable="false"])';
      if(!ListBox.liveOwner(doc,view)||!(root instanceof view.HTMLElement)||!root.isConnected||root.isContentEditable||root.matches(interactive)||!Array.isArray(options)||options.length<1||options.length>256||new Set(options).size!==options.length||
        root.children.length!==options.length||options.some((node,index)=>!(node instanceof view.HTMLElement)||node.ownerDocument!==doc||!node.isConnected||root.children[index]!==node||node.isContentEditable||node.matches(interactive)||node.querySelector(interactive+', [tabindex]')||!node.id||node.id.length>128||/\s/u.test(node.id)||doc.getElementById(node.id)!==node)||
        new Set(options.map(node=>node.id)).size!==options.length||typeof label!=='string'||label.trim()===''||label.length>256||typeof multiple!=='boolean'||!Number.isInteger(active)||active<0||active>=options.length||
        (onSelection!==null&&typeof onSelection!=='function')||listRoots.has(root))throw new TypeError('Invalid local listbox');
      const selection=ListBox.checkedSelection(selected,options.length,multiple);if(selection===null)throw new TypeError('Invalid listbox initial selection');
      Object.assign(this,{root,options:options.slice(),ids:options.map(node=>node.id),document:doc,view,label,multiple,selected:selection,active,onSelection});
      this.disposed=false;this.busy=false;this.error=null;this.anchor=null;this.eligibility=null;this.attributes=[];this.listeners=[];
      for(const [node,names] of [[root,['role','aria-label','aria-labelledby','aria-multiselectable','aria-orientation','aria-activedescendant','tabindex']],...options.map(node=>[node,['role','aria-selected','tabindex']])])for(const name of names){const value=node.getAttribute(name);if(value!==null&&value.length>4096)throw new TypeError('Listbox original attribute capacity');this.attributes.push([node,name,value]);}
      listRoots.set(root,this);this.write(root,'role','listbox');this.write(root,'aria-label',label);this.write(root,'aria-labelledby',null);this.write(root,'aria-multiselectable',String(multiple));this.write(root,'aria-orientation','vertical');
      for(const node of options){this.write(node,'role','option');this.write(node,'tabindex','-1');}
      this.listen(root,'keydown',event=>this.key(event));this.listen(root,'keyup',event=>{if(event.key==='Shift'||!event.shiftKey)this.anchor=null;});this.listen(root,'click',event=>this.click(event));
      this.listen(view,'blur',()=>{this.anchor=null;});this.listen(doc,'visibilitychange',()=>{if(doc.hidden)this.anchor=null;});
      this.observer=new view.MutationObserver(()=>this.refresh());this.observer.observe(doc,{subtree:true,childList:true,attributes:true,attributeFilter:['id','hidden','inert','aria-disabled','aria-hidden','disabled','style','class','tabindex','contenteditable','href','controls','role','aria-label','aria-labelledby','aria-orientation','aria-multiselectable','aria-activedescendant','aria-selected']});this.refresh();
    }
    static checkedSelection(indices,count,multiple){if(!Array.isArray(indices))return null;const size=indices.length;if(!Number.isInteger(size)||size<0||size>count||(!multiple&&size>1))return null;const copied=[],seen=new Set();for(let at=0;at<size;at++){const index=indices[at];if(!Number.isInteger(index)||index<0||index>=count||seen.has(index))return null;seen.add(index);copied.push(index);}return copied.sort((a,b)=>a-b);}
    write(node,name,value){if(this.disposed)return;if(node.getAttribute(name)===value)return;if(value===null)node.removeAttribute(name);else node.setAttribute(name,value);}
    listen(node,type,handler){node.addEventListener(type,handler);this.listeners.push([node,type,handler]);}
    static windowOwner(doc,view){try{return !!view&&doc.defaultView===view&&view.document===doc&&!view.closed;}catch{return false;}}
    static liveOwner(doc,view){try{return ListBox.windowOwner(doc,view)&&(!view.frameElement||view.frameElement.isConnected);}catch{return false;}}
    ownerLive(){return ListBox.liveOwner(this.document,this.view);}
    ownerWindow(){return ListBox.windowOwner(this.document,this.view);}
    valid(){return this.ownerLive()&&!this.root.isContentEditable&&!this.root.matches('button,input,select,textarea,a[href],area[href],summary,iframe,object,embed,audio[controls],video[controls],[contenteditable]:not([contenteditable="false"])')&&!this.view.closed&&this.root.isConnected&&this.root.ownerDocument===this.document&&this.root.children.length===this.options.length&&this.options.every((node,index)=>node.isConnected&&node.ownerDocument===this.document&&this.root.children[index]===node&&node.id===this.ids[index]&&this.document.getElementById(this.ids[index])===node&&!node.isContentEditable&&!node.matches('button,input,select,textarea,a[href],area[href],summary,iframe,object,embed,audio[controls],video[controls],[contenteditable]:not([contenteditable="false"])')&&!node.querySelector('button,input,select,textarea,a[href],area[href],summary,iframe,object,embed,audio[controls],video[controls],[contenteditable]:not([contenteditable="false"]),[tabindex]'));}
    eligible(node){return !node.closest('[hidden],[inert],[aria-hidden="true"],[aria-disabled="true"],[disabled]')&&this.view.getComputedStyle(node).visibility==='visible'&&node.getClientRects().length>0;}
    eligibleIndices(){return this.options.map((node,index)=>this.eligible(node)?index:-1).filter(index=>index>=0);}
    sync(){
      if(!this.valid()){this.error='Ownership';this.dispose();return null;}
      this.write(this.root,'role','listbox');this.write(this.root,'aria-label',this.label);this.write(this.root,'aria-labelledby',null);this.write(this.root,'aria-multiselectable',String(this.multiple));this.write(this.root,'aria-orientation','vertical');for(const node of this.options){this.write(node,'role','option');this.write(node,'tabindex','-1');}
      if(this.disposed)return null;if(!this.valid()){this.error='Ownership';this.dispose();return null;}const eligible=this.eligibleIndices();
      const signature=eligible.join(',');if(this.eligibility!==signature){this.anchor=null;this.eligibility=signature;}
      if(!eligible.includes(this.active))this.active=eligible.find(index=>index>=this.active)??eligible[0]??-1;
      for(let index=0;index<this.options.length;index++)this.write(this.options[index],'aria-selected',String(this.selected.includes(index)));
      const ready=this.eligible(this.root)&&eligible.length>0;this.write(this.root,'tabindex',ready?'0':'-1');this.write(this.root,'aria-activedescendant',ready?this.ids[this.active]:null);
      if(this.disposed)return null;if(!this.valid()){this.error='Ownership';this.dispose();return null;}return {eligible,ready};
    }
    refresh(){if(this.disposed||this.busy)return false;this.busy=true;try{const state=this.sync();if(!state)return false;this.error=state.ready?null:'Unavailable';return state.ready;}finally{this.busy=false;}}
    snapshot(source=null){const unavailable=this.selected.filter(index=>!this.eligible(this.options[index]));return Object.freeze({selectedIndices:Object.freeze(this.selected.slice()),selectedIds:Object.freeze(this.selected.map(index=>this.ids[index])),unavailableSelectedIds:Object.freeze(unavailable.map(index=>this.ids[index])),activeIndex:this.active,...(source?{source}:{})});}
    selection(){if(this.disposed||!this.valid())return null;return this.snapshot();}
    activeIndex(){return this.active;}
    lastError(){return this.error;}
    setSelected(indices,{notify=false}={}){if(this.disposed||this.busy||typeof notify!=='boolean')return false;const checked=ListBox.checkedSelection(indices,this.options.length,this.multiple);if(checked===null)return false;return this.commit(checked,this.active,notify?'programmatic':null,true);}
    setActive(index,{focus=false}={}){if(this.disposed||this.busy||!Number.isInteger(index)||index<0||index>=this.options.length||typeof focus!=='boolean')return false;return this.commit(this.selected.slice(),index,null,true,focus,true);}
    commit(selection,active,source,clearAnchor=false,focus=false,requireActive=false){
      if(this.disposed||this.busy)return false;if(!this.valid()){this.anchor=null;this.error='Ownership';this.dispose();return false;}
      const eligible=this.eligibleIndices();if(selection.some(index=>!this.selected.includes(index)&&!eligible.includes(index)))return false;
      if(active!==-1&&!eligible.includes(active)){if(requireActive){this.anchor=null;return false;}active=eligible.find(index=>index>=active)??eligible[0]??-1;}
      const changed=selection.length!==this.selected.length||selection.some((index,at)=>index!==this.selected[at]);
      this.busy=true;let phase=focus?'Focus':'Lifecycle';
      try{
        if(focus){this.root.focus({preventScroll:true});if(this.disposed)return false;if(!this.valid()){this.error='Ownership';this.dispose();return false;}if(this.document.activeElement!==this.root||!this.eligible(this.root)||!this.eligible(this.options[active])||selection.some(index=>!this.selected.includes(index)&&!this.eligible(this.options[index]))){this.anchor=null;this.sync();return false;}}
        phase='Lifecycle';this.selected=selection.slice();this.active=active;if(clearAnchor)this.anchor=null;const state=this.sync();if(!state)return false;this.error=state.ready?null:'Unavailable';
        if(changed&&source&&this.onSelection){phase='Callback';this.onSelection(this.snapshot(source));phase='Lifecycle';}
        if(this.disposed)return false;const final=this.sync();if(!final)return false;this.error=final.ready?null:'Unavailable';if((focus||source==='keyboard'||source==='pointer')&&(!final.ready||this.document.activeElement!==this.root)){if(final.ready)this.error='Focus';return false;}return true;
      }catch{this.anchor=null;this.error=phase;if(!this.disposed)this.sync();this.error=phase;return false;}finally{this.busy=false;}
    }
    range(target){const state=this.anchor??{index:this.active,baseline:this.selected.slice()};this.anchor=state;const low=Math.min(state.index,target),high=Math.max(state.index,target);return [...new Set([...state.baseline,...this.eligibleIndices().filter(index=>index>=low&&index<=high)])].sort((a,b)=>a-b);}
    key(event){
      if(event.defaultPrevented||event.isComposing||event.altKey||this.disposed||this.busy||this.document.activeElement!==this.root)return;
      const keys=['ArrowUp','ArrowDown','Home','End',' '];if(!keys.includes(event.key))return;if(!this.refresh())return;const eligible=this.eligibleIndices(),position=eligible.indexOf(this.active);
      let target=this.active;if(event.key==='Home')target=eligible[0];else if(event.key==='End')target=eligible.at(-1);else if(event.key==='ArrowUp')target=eligible[Math.max(0,position-1)];else if(event.key==='ArrowDown')target=eligible[Math.min(eligible.length-1,position+1)];
      if(event.key===' '&&(event.ctrlKey||event.metaKey))return;
      event.preventDefault();let selection=this.selected.slice();
      if(this.multiple){if(event.key===' ')selection=selection.includes(target)?selection.filter(index=>index!==target):[...selection,target].sort((a,b)=>a-b);else if(event.shiftKey)selection=this.range(target);}
      else if(!event.ctrlKey&&!event.metaKey)selection=[target];
      this.commit(selection,target,'keyboard',event.key===' '||!event.shiftKey);
    }
    click(event){
      if(event.defaultPrevented||event.button!==0||event.altKey||this.disposed||this.busy||!this.refresh())return;const target=this.options.findIndex(node=>event.target===node||node.contains(event.target));if(target<0||!this.eligible(this.options[target]))return;
      let selection=[target];if(this.multiple){if(event.shiftKey)selection=this.range(target);else if(event.ctrlKey||event.metaKey)selection=this.selected.includes(target)?this.selected.filter(index=>index!==target):[...this.selected,target].sort((a,b)=>a-b);}
      event.preventDefault();this.commit(selection,target,'pointer',!event.shiftKey,true);
    }
    dispose(){if(this.disposed)return;this.disposed=true;this.onSelection=null;this.anchor=null;this.observer?.disconnect();for(const [node,type,handler] of this.listeners){if(node===this.view&&!this.ownerWindow())continue;node.removeEventListener(type,handler);}this.listeners=[];for(const [node,name,value] of this.attributes){if(value===null)node.removeAttribute(name);else node.setAttribute(name,value);}this.attributes=[];if(listRoots.get(this.root)===this)listRoots.delete(this.root);}
  }
  const roots=new WeakMap(),members=new WeakMap();
  const interactive='button,input,select,textarea,a[href],area[href],summary,iframe,object,embed,audio[controls],video[controls],[contenteditable]:not([contenteditable="false"])';
  class TreeView {
    constructor(root,entries,{label,multiple=false,selected=[],active=0,expanded=[],onSelection=null,onExpansion=null}={}){
      const doc=root?.ownerDocument,view=doc?.defaultView;
      if(!view||view.closed||!(root instanceof view.HTMLElement)||!root.isConnected||!Array.isArray(entries)||entries.length<1||entries.length>256||typeof label!=='string'||!label.trim()||label.length>256||typeof multiple!=='boolean'||!Number.isInteger(active)||active<0||active>=entries.length||(onSelection!==null&&typeof onSelection!=='function')||(onExpansion!==null&&typeof onExpansion!=='function')||roots.has(root)||members.has(root))throw new TypeError('Invalid local tree');
      const copied=entries.map(entry=>{if(!entry||typeof entry!=='object')throw new TypeError('Invalid tree entry');return {node:entry.node,label:entry.label,parent:entry.parent,group:entry.group??null,toggle:entry.toggle??null};});
      Object.assign(this,{root,entries:copied,document:doc,view,label,multiple,active,onSelection,onExpansion});
      this.disposed=false;this.busy=false;this.error=null;this.anchor=null;this.eligibility=null;this.attributes=[];this.styles=[];this.listeners=[];this.depths=[];this.children=copied.map(()=>[]);this.ids=[];this.groupIds=[];this.known=new Set([root]);
      for(let index=0;index<copied.length;index++){
        const entry=copied[index];if((entry.parent!==null&&(!Number.isInteger(entry.parent)||entry.parent<0||entry.parent>=index))||typeof entry.label!=='string'||!entry.label.trim()||entry.label.length>256)throw new TypeError('Invalid tree parent or name');
        this.depths[index]=entry.parent===null?1:this.depths[entry.parent]+1;if(this.depths[index]>32)throw new TypeError('Tree depth capacity');
        if(entry.parent!==null)this.children[entry.parent].push(index);
        for(const node of [entry.node,entry.group,entry.toggle].filter(Boolean)){if(!(node instanceof view.HTMLElement)||node.ownerDocument!==doc||!node.isConnected||this.known.has(node)||members.has(node)||roots.has(node))throw new TypeError('Invalid tree member');this.known.add(node);}
        this.ids.push(entry.node?.id);this.groupIds.push(entry.group?.id??null);
      }
      this.top=copied.map((entry,index)=>entry.parent===null?index:-1).filter(index=>index>=0);
      const preorder=[];const visit=index=>{preorder.push(index);for(const child of this.children[index])visit(child);};for(const index of this.top)visit(index);
      if(preorder.some((index,at)=>index!==at)||!this.valid())throw new TypeError('Invalid tree hierarchy');
      this.selected=TreeView.checked(selected,copied.length,multiple);this.expanded=TreeView.checked(expanded,copied.length,true);
      if(this.selected===null||this.expanded===null||this.expanded.some(index=>!copied[index].group))throw new TypeError('Invalid initial tree model');
      const capture=(node,names)=>{for(const name of names){const value=node.getAttribute(name);if(value!==null&&value.length>4096)throw new TypeError('Tree original attribute capacity');this.attributes.push([node,name,value]);}};
      capture(root,['role','aria-label','aria-labelledby','aria-multiselectable','aria-activedescendant','tabindex']);
      for(const entry of copied){capture(entry.node,['role','aria-label','aria-labelledby','aria-level','aria-posinset','aria-setsize','aria-selected','aria-expanded','tabindex']);if(entry.group){capture(entry.group,['role','hidden','tabindex']);capture(entry.toggle,['role','aria-label','aria-labelledby','aria-controls','aria-expanded','tabindex']);const raw=entry.group.getAttribute('style'),value=entry.group.style.getPropertyValue('display'),priority=entry.group.style.getPropertyPriority('display');if((raw?.length??0)>16384||value.length>4096)throw new TypeError('Tree original style capacity');this.styles.push({node:entry.group,raw,value,priority,normalized:entry.group.style.cssText});}}
      roots.set(root,this);for(const node of this.known)if(node!==root)members.set(node,this);
      this.listen(root,'keydown',event=>this.key(event));this.listen(root,'click',event=>this.click(event));this.listen(root,'keyup',event=>{if(event.key==='Shift'||!event.shiftKey)this.anchor=null;});this.listen(view,'blur',()=>{this.anchor=null;});this.listen(doc,'visibilitychange',()=>{if(doc.hidden)this.anchor=null;});
      this.observer=new view.MutationObserver(()=>this.refresh());this.observer.observe(doc,{subtree:true,childList:true,attributes:true,attributeFilter:['id','hidden','inert','aria-disabled','aria-hidden','disabled','style','class','tabindex','contenteditable','href','controls','type','role','aria-label','aria-labelledby','aria-multiselectable','aria-activedescendant','aria-selected','aria-expanded','aria-level','aria-posinset','aria-setsize','aria-controls']});this.refresh();
    }
    static checked(indices,count,multiple){if(!Array.isArray(indices))return null;const size=indices.length;if(!Number.isInteger(size)||size<0||size>count||(!multiple&&size>1))return null;const copied=[],seen=new Set();for(let at=0;at<size;at++){const index=indices[at];if(!Number.isInteger(index)||index<0||index>=count||seen.has(index))return null;seen.add(index);copied.push(index);}return copied.sort((a,b)=>a-b);}
    valid(){try{
      if(this.document.defaultView!==this.view||this.view.document!==this.document||(this.view.frameElement&&!this.view.frameElement.isConnected)||this.view.closed||!this.root.isConnected||this.root.ownerDocument!==this.document||this.root.isContentEditable||this.root.matches(interactive)||this.root.children.length!==this.top.length||this.top.some((index,at)=>this.root.children[at]!==this.entries[index].node))return false;
      const ids=new Set();
      for(let index=0;index<this.entries.length;index++){
        const entry=this.entries[index],node=entry.node;if(!node.isConnected||node.ownerDocument!==this.document||node.isContentEditable||node.matches(interactive)||node.id!==this.ids[index]||!node.id||node.id.length>128||/\s/u.test(node.id)||ids.has(node.id)||this.document.getElementById(node.id)!==node)return false;ids.add(node.id);
        if(this.children[index].length){
          const group=entry.group,toggle=entry.toggle;if(!group||!toggle||group.parentElement!==node||toggle.parentElement!==node||!group.isConnected||group.ownerDocument!==this.document||group.isContentEditable||group.matches(interactive)||group.id!==this.groupIds[index]||!group.id||group.id.length>128||/\s/u.test(group.id)||ids.has(group.id)||this.document.getElementById(group.id)!==group||group.children.length!==this.children[index].length||this.children[index].some((child,at)=>group.children[at]!==this.entries[child].node)||toggle.ownerDocument!==this.document||!toggle.isConnected||toggle.localName!=='button'||toggle.getAttribute('type')!=='button'||toggle.isContentEditable)return false;ids.add(group.id);
        }else if(entry.group||entry.toggle)return false;
      }
      const walker=this.document.createTreeWalker(this.root,1);let node,count=0;while((node=walker.nextNode())){if(++count>8192)return false;const entry=this.entries.find(entry=>entry.toggle===node);if(node.isContentEditable)return false;if(entry){if(node.localName!=='button'||node.getAttribute('type')!=='button')return false;}else if(node.matches(interactive)||(!this.known.has(node)&&node.hasAttribute('tabindex')))return false;}
      return true;
    }catch{return false;}}
    write(node,name,value){if(this.disposed||node.getAttribute(name)===value)return;if(value===null)node.removeAttribute(name);else node.setAttribute(name,value);}
    profile(node,pairs){for(const [name,value] of pairs){this.write(node,name,value);if(this.disposed)return false;}return true;}
    listen(node,type,handler){node.addEventListener(type,handler);this.listeners.push([node,type,handler]);}
    eligible(node){return !node.closest('[hidden],[inert],[aria-hidden="true"],[aria-disabled="true"],[disabled]')&&this.view.getComputedStyle(node).visibility==='visible'&&node.getClientRects().length>0;}
    visible(index){for(let parent=this.entries[index].parent;parent!==null;parent=this.entries[parent].parent)if(!this.expanded.includes(parent))return false;return true;}
    eligibleIndices(){return this.entries.map((entry,index)=>this.visible(index)&&this.eligible(entry.node)?index:-1).filter(index=>index>=0);}
    toggleEligible(index){const entry=this.entries[index];return !!entry.toggle&&this.visible(index)&&this.eligible(entry.node)&&this.eligible(entry.toggle);}
    fallback(eligible){if(eligible.includes(this.active))return;for(let parent=this.active<0?null:this.entries[this.active].parent;parent!==null;parent=this.entries[parent].parent)if(eligible.includes(parent)){this.active=parent;return;}this.active=eligible.find(index=>index>=this.active)??eligible[0]??-1;}
    sync(){
      if(!this.valid()){this.error='Ownership';this.dispose();return null;}
      if(!this.profile(this.root,[['role','tree'],['aria-label',this.label],['aria-labelledby',null],['aria-multiselectable',String(this.multiple)]]))return null;
      for(let index=0;index<this.entries.length;index++){
        const entry=this.entries[index],siblings=entry.parent===null?this.top:this.children[entry.parent],opened=this.expanded.includes(index);
        if(!this.profile(entry.node,[['role','treeitem'],['aria-label',entry.label],['aria-labelledby',null],['aria-level',String(this.depths[index])],['aria-posinset',String(siblings.indexOf(index)+1)],['aria-setsize',String(siblings.length)],['aria-selected',String(this.selected.includes(index))],['aria-expanded',entry.group?String(opened):null],['tabindex','-1']]))return null;
        if(entry.group){if(!this.profile(entry.group,[['role','group'],['hidden',opened?null:''],['tabindex','-1']]))return null;const style=this.styles.find(style=>style.node===entry.group),value=opened?style.value:'none',priority=opened?style.priority:'important';if(entry.group.style.getPropertyValue('display')!==value||entry.group.style.getPropertyPriority('display')!==priority){if(value)entry.group.style.setProperty('display',value,priority);else entry.group.style.removeProperty('display');}if(this.disposed)return null;
          if(!this.profile(entry.toggle,[['role','button'],['aria-label',`${opened?'Collapse':'Expand'} ${entry.label}`],['aria-labelledby',null],['aria-controls',entry.group.id],['aria-expanded',String(opened)],['tabindex','-1']]))return null;
        }
      }
      if(this.disposed)return null;if(!this.valid()){this.error='Ownership';this.dispose();return null;}
      const eligible=this.eligibleIndices(),signature=eligible.join(',')+';'+this.expanded.join(',');if(signature!==this.eligibility){this.anchor=null;this.eligibility=signature;}this.fallback(eligible);
      const ready=this.eligible(this.root)&&eligible.length>0;if(!this.profile(this.root,[['tabindex',ready?'0':'-1'],['aria-activedescendant',ready?this.ids[this.active]:null]]))return null;if(!this.valid()){this.error='Ownership';this.dispose();return null;}return {eligible,ready};
    }
    refresh(){if(this.disposed||this.busy)return false;this.busy=true;try{const state=this.sync();if(!state)return false;this.error=state.ready?null:'Unavailable';return state.ready;}finally{this.busy=false;}}
    snapshot(source=null){const eligible=this.eligibleIndices();return Object.freeze({selectedIndices:Object.freeze(this.selected.slice()),selectedIds:Object.freeze(this.selected.map(index=>this.ids[index])),unavailableSelectedIds:Object.freeze(this.selected.filter(index=>!eligible.includes(index)).map(index=>this.ids[index])),activeIndex:this.active,...(source?{source}:{})});}
    expandedSnapshot(source=null){return Object.freeze({expandedIndices:Object.freeze(this.expanded.slice()),expandedIds:Object.freeze(this.expanded.map(index=>this.ids[index])),activeIndex:this.active,...(source?{source}:{})});}
    selection(){return this.disposed||!this.valid()?null:this.snapshot();}
    expansion(){return this.disposed||!this.valid()?null:this.expandedSnapshot();}
    activeIndex(){return this.active;}
    lastError(){return this.error;}
    setSelected(indices,{notify=false}={}){if(this.disposed||this.busy||typeof notify!=='boolean')return false;const selected=TreeView.checked(indices,this.entries.length,this.multiple);return selected===null?false:this.commit({selected,active:this.active,source:notify?'programmatic':null,clearAnchor:true});}
    setExpanded(indices,{notify=false}={}){if(this.disposed||this.busy||typeof notify!=='boolean')return false;const expanded=TreeView.checked(indices,this.entries.length,true);return expanded===null||expanded.some(index=>!this.entries[index].group)?false:this.commit({expanded,active:this.active,source:notify?'programmatic':null,clearAnchor:true});}
    setActive(index,{focus=false}={}){if(this.disposed||this.busy||!Number.isInteger(index)||index<0||index>=this.entries.length||typeof focus!=='boolean')return false;return this.commit({active:index,focus,requireActive:true,clearAnchor:true});}
    commit({selected=this.selected.slice(),expanded=this.expanded.slice(),active=this.active,source=null,clearAnchor=false,focus=false,requireActive=false,toggle=null}={}){
      if(this.disposed||this.busy)return false;if(!this.valid()){this.error='Ownership';this.dispose();return false;}const eligible=this.eligibleIndices();if(selected.some(index=>!this.selected.includes(index)&&!eligible.includes(index)))return false;if(requireActive&&!eligible.includes(active)){this.anchor=null;return false;}if(toggle!==null&&!this.toggleEligible(toggle))return false;
      const selectionChanged=selected.length!==this.selected.length||selected.some((index,at)=>index!==this.selected[at]),expansionChanged=expanded.length!==this.expanded.length||expanded.some((index,at)=>index!==this.expanded[at]);this.busy=true;let phase=focus?'Focus':'Lifecycle';
      try{
        if(focus){this.root.focus({preventScroll:true});if(this.disposed)return false;if(!this.valid()){this.error='Ownership';this.dispose();return false;}const current=this.eligibleIndices();if(this.document.activeElement!==this.root||!this.eligible(this.root)||(requireActive&&!current.includes(active))||(toggle!==null&&!this.toggleEligible(toggle))||selected.some(index=>!this.selected.includes(index)&&!current.includes(index))){this.anchor=null;this.sync();this.error='Focus';return false;}}
        phase='Lifecycle';this.selected=selected.slice();this.expanded=expanded.slice();this.active=active;if(clearAnchor)this.anchor=null;const state=this.sync();if(!state)return false;this.error=state.ready?null:'Unavailable';
        if(selectionChanged&&source&&this.onSelection){phase='Callback';this.onSelection(this.snapshot(source));phase='Lifecycle';}
        if(this.disposed)return false;if(!this.valid()){this.error='Ownership';this.dispose();return false;}
        if(expansionChanged&&source&&this.onExpansion){phase='Callback';this.onExpansion(this.expandedSnapshot(source));phase='Lifecycle';}
        if(this.disposed)return false;const final=this.sync();if(!final)return false;this.error=final.ready?null:'Unavailable';if(toggle!==null&&!this.toggleEligible(toggle)){this.error='Unavailable';return false;}if((focus||source==='keyboard'||source==='pointer')&&(!final.ready||this.document.activeElement!==this.root)){if(final.ready)this.error='Focus';return false;}return true;
      }catch{this.anchor=null;this.error=phase;if(!this.disposed)this.sync();this.error=phase;return false;}finally{this.busy=false;}
    }
    range(target){const state=this.anchor??{index:this.active,baseline:this.selected.slice()};this.anchor=state;const low=Math.min(state.index,target),high=Math.max(state.index,target);return [...new Set([...state.baseline,...this.eligibleIndices().filter(index=>index>=low&&index<=high)])].sort((a,b)=>a-b);}
    key(event){
      if(event.defaultPrevented||event.isComposing||event.altKey||this.disposed||this.busy||this.document.activeElement!==this.root)return;const keys=['ArrowUp','ArrowDown','Home','End','ArrowLeft','ArrowRight',' '];if(!keys.includes(event.key)||((event.ctrlKey||event.metaKey)&&[' ','ArrowLeft','ArrowRight'].includes(event.key)))return;if(!this.refresh())return;
      const eligible=this.eligibleIndices(),index=this.active,entry=this.entries[index];let target=index;
      if(event.key==='ArrowRight'||event.key==='ArrowLeft'){
        if(entry.group&&(event.key==='ArrowRight'?!this.expanded.includes(index):this.expanded.includes(index))){if(!this.toggleEligible(index))return;event.preventDefault();const expanded=this.expanded.includes(index)?this.expanded.filter(at=>at!==index):[...this.expanded,index].sort((a,b)=>a-b);this.commit({expanded,source:'keyboard',clearAnchor:true,toggle:index});return;}
        if(event.key==='ArrowRight')target=this.children[index].find(child=>eligible.includes(child))??index;else for(let parent=entry.parent;parent!==null;parent=this.entries[parent].parent)if(eligible.includes(parent)){target=parent;break;}
      }else if(event.key==='Home')target=eligible[0];else if(event.key==='End')target=eligible.at(-1);else if(event.key==='ArrowUp')target=eligible[Math.max(0,eligible.indexOf(index)-1)];else if(event.key==='ArrowDown')target=eligible[Math.min(eligible.length-1,eligible.indexOf(index)+1)];
      event.preventDefault();if(event.key!==' '&&target===index){if(!event.shiftKey||['ArrowLeft','ArrowRight'].includes(event.key))this.anchor=null;return;}let selected=this.selected.slice();if(this.multiple){if(event.key===' ')selected=selected.includes(target)?selected.filter(at=>at!==target):[...selected,target].sort((a,b)=>a-b);else if(event.shiftKey&&!['ArrowLeft','ArrowRight'].includes(event.key))selected=this.range(target);}else if(!event.ctrlKey&&!event.metaKey&&(target!==index||event.key===' '))selected=[target];this.commit({selected,active:target,source:'keyboard',clearAnchor:event.key===' '||!event.shiftKey||['ArrowLeft','ArrowRight'].includes(event.key)});
    }
    click(event){
      if(event.defaultPrevented||event.button!==0||event.altKey||this.disposed||this.busy||!this.refresh())return;let target=event.target;if(!(target instanceof this.view.Element))target=target?.parentElement;let index=-1,toggle=null;for(let node=target;node&&node!==this.root;node=node.parentElement){const found=this.entries.findIndex(entry=>entry.node===node);if(found>=0){index=found;break;}const at=this.entries.findIndex(entry=>entry.toggle===node);if(at>=0)toggle=at;if(this.entries.some(entry=>entry.group===node))return;}
      if(index<0||!this.eligibleIndices().includes(index))return;if(toggle!==null){if(toggle!==index||!this.toggleEligible(index))return;event.preventDefault();const expanded=this.expanded.includes(index)?this.expanded.filter(at=>at!==index):[...this.expanded,index].sort((a,b)=>a-b);this.commit({expanded,active:index,source:'pointer',clearAnchor:true,focus:true,requireActive:true,toggle:index});return;}
      let selected=[index];if(this.multiple){if(event.shiftKey)selected=this.range(index);else if(event.ctrlKey||event.metaKey)selected=this.selected.includes(index)?this.selected.filter(at=>at!==index):[...this.selected,index].sort((a,b)=>a-b);}event.preventDefault();this.commit({selected,active:index,source:'pointer',clearAnchor:!event.shiftKey,focus:true,requireActive:true});
    }
    ownerWindow(){return ListBox.windowOwner(this.document,this.view);}
    dispose(){if(this.disposed)return;this.disposed=true;this.onSelection=null;this.onExpansion=null;this.anchor=null;this.observer?.disconnect();for(const [node,type,handler] of this.listeners){if(node===this.view&&!this.ownerWindow())continue;node.removeEventListener(type,handler);}this.listeners=[];for(const [node,name,value] of this.attributes){if(value===null)node.removeAttribute(name);else node.setAttribute(name,value);}this.attributes=[];for(const style of this.styles){if(style.value)style.node.style.setProperty('display',style.value,style.priority);else style.node.style.removeProperty('display');if(style.node.style.cssText===style.normalized){if(style.raw===null){if(style.node.getAttribute('style')!==null)style.node.removeAttribute('style');}else style.node.setAttribute('style',style.raw);}}this.styles=[];if(roots.get(this.root)===this)roots.delete(this.root);for(const node of this.known)if(members.get(node)===this)members.delete(node);}
  }
  const tableRoots=new WeakMap(),tableMembers=new WeakMap();
  const gridInteractive='button,input,select,textarea,a[href],area[href],summary,iframe,object,embed,audio[controls],video[controls],[contenteditable]:not([contenteditable="false"])';
  class TableView {
    constructor(table,headers,rows,{label,multiple=false,selected=[],active={row:0,column:0},onSelection=null}={}){
      const doc=table?.ownerDocument,view=doc?.defaultView;
      if(!view||view.closed||!(table instanceof view.HTMLTableElement)||!table.isConnected||!Array.isArray(headers)||headers.length<1||headers.length>32||!Array.isArray(rows)||rows.length<1||rows.length>256||rows.length*headers.length>4096||typeof label!=='string'||!label.trim()||label.length>256||typeof multiple!=='boolean'||(onSelection!==null&&typeof onSelection!=='function')||tableRoots.has(table)||tableMembers.has(table))throw new TypeError('Invalid local readonly grid');
      const copiedHeaders=headers.map(entry=>({node:entry?.node,label:entry?.label})),copiedRows=rows.map(entry=>{if(!Array.isArray(entry?.cells)||entry.cells.length!==headers.length)throw new TypeError('Invalid grid dimensions');return {node:entry.node,cells:entry.cells.map(cell=>({node:cell?.node,label:cell?.label}))};});
      Object.assign(this,{root:table,headers:copiedHeaders,rows:copiedRows,label,multiple,document:doc,view,onSelection});
      this.head=table.children[0];this.body=table.children[1];this.headerRow=this.head?.children[0];this.attributes=[];this.listeners=[];this.disposed=false;this.busy=false;this.error=null;this.anchor=null;this.eligibility=null;this.known=new Set([table]);
      for(const node of [this.head,this.body,this.headerRow,...copiedHeaders.map(entry=>entry.node),...copiedRows.flatMap(entry=>[entry.node,...entry.cells.map(cell=>cell.node)])]){if(!(node instanceof view.HTMLElement)||!node.isConnected||node.ownerDocument!==doc||this.known.has(node)||tableRoots.has(node)||tableMembers.has(node))throw new TypeError('Invalid grid member');this.known.add(node);}
      for(const entry of [...copiedHeaders,...copiedRows.flatMap(row=>row.cells)])if(typeof entry.label!=='string'||!entry.label.trim()||entry.label.length>256)throw new TypeError('Invalid grid label');
      this.headerIds=copiedHeaders.map(entry=>entry.node.id);this.rowIds=copiedRows.map(entry=>entry.node.id);this.cellIds=copiedRows.map(entry=>entry.cells.map(cell=>cell.node.id));
      if(!this.valid())throw new TypeError('Invalid grid profile');
      this.selected=TableView.checked(selected,rows.length,multiple);if(this.selected===null||!this.coordinate(active))throw new TypeError('Invalid initial grid model');this.active={row:active.row,column:active.column};
      const capture=(node,names)=>{for(const name of names){const value=node.getAttribute(name);if(value!==null&&value.length>4096)throw new TypeError('Grid original attribute capacity');this.attributes.push([node,name,value]);}};
      capture(table,['role','aria-label','aria-labelledby','aria-readonly','aria-multiselectable','aria-rowcount','aria-colcount','aria-activedescendant','tabindex']);
      for(const section of [this.head,this.body])capture(section,['role','tabindex']);capture(this.headerRow,['role','aria-rowindex','tabindex']);
      for(const header of copiedHeaders)capture(header.node,['role','aria-label','aria-labelledby','aria-colindex','tabindex']);
      for(const row of copiedRows){capture(row.node,['role','aria-rowindex','aria-selected','tabindex']);for(const cell of row.cells)capture(cell.node,['role','aria-label','aria-labelledby','aria-colindex','headers','tabindex']);}
      tableRoots.set(table,this);for(const node of this.known)if(node!==table)tableMembers.set(node,this);
      this.listen(table,'keydown',event=>this.key(event));this.listen(table,'click',event=>this.click(event));this.listen(table,'keyup',event=>{if(event.key==='Shift'||!event.shiftKey)this.anchor=null;});this.listen(view,'blur',()=>{this.anchor=null;});this.listen(doc,'visibilitychange',()=>{if(doc.hidden)this.anchor=null;});
      this.observer=new view.MutationObserver(()=>this.refresh());this.observer.observe(doc,{subtree:true,childList:true,attributes:true,attributeFilter:['id','hidden','inert','aria-disabled','aria-hidden','disabled','style','class','tabindex','contenteditable','href','controls','scope','colspan','rowspan','role','aria-label','aria-labelledby','aria-readonly','aria-multiselectable','aria-rowcount','aria-colcount','aria-activedescendant','aria-rowindex','aria-colindex','aria-selected','headers']});this.refresh();
    }
    static checked(indices,count,multiple){if(!Array.isArray(indices))return null;const size=indices.length;if(!Number.isInteger(size)||size<0||size>count||(!multiple&&size>1))return null;const copied=[],seen=new Set();for(let at=0;at<size;at++){const index=indices[at];if(!Number.isInteger(index)||index<0||index>=count||seen.has(index))return null;seen.add(index);copied.push(index);}return copied.sort((a,b)=>a-b);}
    coordinate(value){return value&&typeof value==='object'&&Number.isInteger(value.row)&&value.row>=0&&value.row<this.rows.length&&Number.isInteger(value.column)&&value.column>=0&&value.column<this.headers.length;}
    valid(){try{
      if(this.document.defaultView!==this.view||this.view.document!==this.document||this.view.closed||(this.view.frameElement&&!this.view.frameElement.isConnected)||!this.root.isConnected||this.root.ownerDocument!==this.document||this.root.isContentEditable||this.root.children.length!==2||this.root.children[0]!==this.head||this.root.children[1]!==this.body||this.head.localName!=='thead'||this.body.localName!=='tbody'||this.head.children.length!==1||this.head.children[0]!==this.headerRow||this.headerRow.localName!=='tr'||this.headerRow.children.length!==this.headers.length||this.body.children.length!==this.rows.length)return false;
      for(const node of this.known)if(!node.isConnected||node.ownerDocument!==this.document||node.isContentEditable)return false;
      const ids=new Set(),identity=(node,id)=>{if(!id||id.length>128||/\s/u.test(id)||ids.has(id)||node.id!==id||this.document.getElementById(id)!==node)return false;ids.add(id);return true;},cellProfile=node=>node.colSpan===1&&node.rowSpan===1&&[node.getAttribute('colspan'),node.getAttribute('rowspan')].every(value=>value===null||value==='1');
      for(let column=0;column<this.headers.length;column++){const node=this.headers[column].node;if(this.headerRow.children[column]!==node||node.localName!=='th'||node.getAttribute('scope')!=='col'||!cellProfile(node)||!identity(node,this.headerIds[column]))return false;}
      for(let row=0;row<this.rows.length;row++){const entry=this.rows[row];if(this.body.children[row]!==entry.node||entry.node.localName!=='tr'||entry.node.children.length!==this.headers.length||!identity(entry.node,this.rowIds[row]))return false;for(let column=0;column<this.headers.length;column++){const node=entry.cells[column].node;if(entry.node.children[column]!==node||node.localName!=='td'||!cellProfile(node)||!identity(node,this.cellIds[row][column]))return false;}}
      const walker=this.document.createTreeWalker(this.root,1);let node,count=0;while((node=walker.nextNode()))if(++count>16384||node.isContentEditable||node.matches(gridInteractive)||node.localName==='table'||(!this.known.has(node)&&node.hasAttribute('tabindex')))return false;return true;
    }catch{return false;}}
    write(node,name,value){if(this.disposed||node.getAttribute(name)===value)return;if(value===null)node.removeAttribute(name);else node.setAttribute(name,value);}
    profile(node,pairs){for(const [name,value] of pairs){this.write(node,name,value);if(this.disposed)return false;}return true;}
    listen(node,type,handler){node.addEventListener(type,handler);this.listeners.push([node,type,handler]);}
    eligible(node){return !node.closest('[hidden],[inert],[aria-hidden="true"],[aria-disabled="true"],[disabled]')&&this.view.getComputedStyle(node).visibility==='visible'&&node.getClientRects().length>0;}
    eligibleCells(){const columns=this.headers.map(entry=>this.eligible(entry.node)),cells=[];for(let row=0;row<this.rows.length;row++)if(this.eligible(this.rows[row].node))for(let column=0;column<this.headers.length;column++)if(columns[column]&&this.eligible(this.rows[row].cells[column].node))cells.push({row,column});return cells;}
    same(a,b){return a.row===b.row&&a.column===b.column;}
    fallback(cells){if(cells.some(cell=>this.same(cell,this.active)))return;const current=this.active,candidates=cells.filter(cell=>cell.row===current.row),pool=candidates.length?candidates:cells;this.active=pool.slice().sort((a,b)=>Math.abs(a.row-current.row)-Math.abs(b.row-current.row)||Math.abs(a.column-current.column)-Math.abs(b.column-current.column)||b.row-a.row||b.column-a.column)[0]??{row:-1,column:-1};}
    sync(){
      if(!this.valid()){this.error='Ownership';this.dispose();return null;}
      if(!this.profile(this.root,[['role','grid'],['aria-label',this.label],['aria-labelledby',null],['aria-readonly','true'],['aria-multiselectable',String(this.multiple)],['aria-rowcount',String(this.rows.length+1)],['aria-colcount',String(this.headers.length)]]))return null;
      for(const section of [this.head,this.body])if(!this.profile(section,[['role','rowgroup'],['tabindex','-1']]))return null;if(!this.profile(this.headerRow,[['role','row'],['aria-rowindex','1'],['tabindex','-1']]))return null;
      for(let column=0;column<this.headers.length;column++){const header=this.headers[column];if(!this.profile(header.node,[['role','columnheader'],['aria-label',header.label],['aria-labelledby',null],['aria-colindex',String(column+1)],['tabindex','-1']]))return null;}
      for(let row=0;row<this.rows.length;row++){const entry=this.rows[row];if(!this.profile(entry.node,[['role','row'],['aria-rowindex',String(row+2)],['aria-selected',String(this.selected.includes(row))],['tabindex','-1']]))return null;for(let column=0;column<this.headers.length;column++){const cell=entry.cells[column];if(!this.profile(cell.node,[['role','gridcell'],['aria-label',cell.label],['aria-labelledby',null],['aria-colindex',String(column+1)],['headers',this.headerIds[column]],['tabindex','-1']]))return null;}}
      if(!this.valid()){this.error='Ownership';this.dispose();return null;}const cells=this.eligibleCells(),signature=cells.map(cell=>cell.row+':'+cell.column).join(',');if(signature!==this.eligibility){this.anchor=null;this.eligibility=signature;}this.fallback(cells);const ready=this.eligible(this.root)&&cells.length>0;
      if(!this.profile(this.root,[['tabindex',ready?'0':'-1'],['aria-activedescendant',ready?this.cellIds[this.active.row][this.active.column]:null]]))return null;if(!this.valid()){this.error='Ownership';this.dispose();return null;}return {cells,ready};
    }
    refresh(){if(this.disposed||this.busy)return false;this.busy=true;try{const state=this.sync();if(!state)return false;this.error=state.ready?null:'Unavailable';return state.ready;}finally{this.busy=false;}}
    activeCell(){return Object.freeze({...this.active,id:this.active.row<0?null:this.cellIds[this.active.row][this.active.column]});}
    snapshot(source=null){const available=new Set(this.eligibleCells().map(cell=>cell.row));return Object.freeze({selectedIndices:Object.freeze(this.selected.slice()),selectedIds:Object.freeze(this.selected.map(row=>this.rowIds[row])),unavailableSelectedIds:Object.freeze(this.selected.filter(row=>!available.has(row)).map(row=>this.rowIds[row])),activeCell:this.activeCell(),...(source?{source}:{})});}
    selection(){return this.disposed||!this.valid()?null:this.snapshot();}
    lastError(){return this.error;}
    setSelected(indices,{notify=false}={}){if(this.disposed||this.busy||typeof notify!=='boolean')return false;const selected=TableView.checked(indices,this.rows.length,this.multiple);return selected===null?false:this.commit({selected,source:notify?'programmatic':null,clearAnchor:true});}
    setActive(active,{focus=false}={}){if(this.disposed||this.busy||!this.coordinate(active)||typeof focus!=='boolean')return false;return this.commit({active:{row:active.row,column:active.column},focus,requireActive:true,clearAnchor:true});}
    commit({selected=this.selected.slice(),active={...this.active},source=null,clearAnchor=false,focus=false,requireActive=false}={}){
      if(this.disposed||this.busy)return false;if(!this.valid()){this.error='Ownership';this.dispose();return false;}const cells=this.eligibleCells(),rows=new Set(cells.map(cell=>cell.row));if(selected.some(row=>!this.selected.includes(row)&&!rows.has(row)))return false;if(requireActive&&!cells.some(cell=>this.same(cell,active))){this.anchor=null;return false;}const changed=selected.length!==this.selected.length||selected.some((row,index)=>row!==this.selected[index]);this.busy=true;let phase=focus?'Focus':'Lifecycle';
      try{
        if(focus){this.root.focus({preventScroll:true});if(this.disposed)return false;if(!this.valid()){this.error='Ownership';this.dispose();return false;}const current=this.eligibleCells(),available=new Set(current.map(cell=>cell.row));if(this.document.activeElement!==this.root||!this.eligible(this.root)||(requireActive&&!current.some(cell=>this.same(cell,active)))||selected.some(row=>!this.selected.includes(row)&&!available.has(row))){this.anchor=null;this.sync();this.error='Focus';return false;}}
        phase='Lifecycle';this.selected=selected.slice();this.active={...active};if(clearAnchor)this.anchor=null;const state=this.sync();if(!state)return false;this.error=state.ready?null:'Unavailable';if(changed&&source&&this.onSelection){phase='Callback';this.onSelection(this.snapshot(source));phase='Lifecycle';}if(this.disposed)return false;const final=this.sync();if(!final)return false;this.error=final.ready?null:'Unavailable';if((focus||source==='keyboard'||source==='pointer')&&(!final.ready||this.document.activeElement!==this.root)){if(final.ready)this.error='Focus';return false;}return true;
      }catch{this.anchor=null;this.error=phase;if(!this.disposed)this.sync();this.error=phase;return false;}finally{this.busy=false;}
    }
    range(row){const anchor=this.anchor??{row:this.active.row,baseline:this.selected.slice()};this.anchor=anchor;const low=Math.min(anchor.row,row),high=Math.max(anchor.row,row);return [...new Set([...anchor.baseline,...this.eligibleCells().map(cell=>cell.row).filter(at=>at>=low&&at<=high)])].sort((a,b)=>a-b);}
    key(event){
      if(event.defaultPrevented||event.isComposing||event.altKey||this.disposed||this.busy||this.document.activeElement!==this.root)return;if(!['ArrowUp','ArrowDown','ArrowLeft','ArrowRight','Home','End',' '].includes(event.key)||(event.key===' '&&(event.ctrlKey||event.metaKey)))return;if(!this.refresh())return;const cells=this.eligibleCells(),current=this.active;let target=current;
      if(event.key==='Home'||event.key==='End'){const pool=(event.ctrlKey||event.metaKey)?cells:cells.filter(cell=>cell.row===current.row);target=event.key==='Home'?pool[0]:pool.at(-1);}
      else if(event.key==='ArrowLeft'||event.key==='ArrowRight'){const pool=cells.filter(cell=>cell.row===current.row),at=pool.findIndex(cell=>this.same(cell,current));target=pool[event.key==='ArrowLeft'?Math.max(0,at-1):Math.min(pool.length-1,at+1)];}
      else if(event.key==='ArrowUp'||event.key==='ArrowDown'){const pool=cells.filter(cell=>cell.column===current.column),at=pool.findIndex(cell=>this.same(cell,current));target=pool[event.key==='ArrowUp'?Math.max(0,at-1):Math.min(pool.length-1,at+1)];}
      const range=event.shiftKey&&!event.ctrlKey&&!event.metaKey&&['ArrowUp','ArrowDown'].includes(event.key);event.preventDefault();if(event.key!==' '&&this.same(target,current)){if(!range)this.anchor=null;return;}let selected=this.selected.slice();
      if(this.multiple){if(event.key===' ')selected=selected.includes(target.row)?selected.filter(row=>row!==target.row):[...selected,target.row].sort((a,b)=>a-b);else if(range)selected=this.range(target.row);}else if(!event.ctrlKey&&!event.metaKey&&(event.key===' '||target.row!==current.row))selected=[target.row];this.commit({selected,active:target,source:'keyboard',clearAnchor:!range});
    }
    click(event){
      if(event.defaultPrevented||event.button!==0||event.altKey||this.disposed||this.busy||!this.refresh())return;let node=event.target;if(!(node instanceof this.view.Element))node=node?.parentElement;let active=null;for(;node&&node!==this.root;node=node.parentElement){for(let row=0;row<this.rows.length&&!active;row++){const column=this.rows[row].cells.findIndex(cell=>cell.node===node);if(column>=0)active={row,column};}if(active)break;}
      if(!active||!this.eligibleCells().some(cell=>this.same(cell,active)))return;let selected=[active.row];if(this.multiple){if(event.shiftKey&&!event.ctrlKey&&!event.metaKey)selected=this.range(active.row);else if(event.ctrlKey||event.metaKey)selected=this.selected.includes(active.row)?this.selected.filter(row=>row!==active.row):[...this.selected,active.row].sort((a,b)=>a-b);}event.preventDefault();this.commit({selected,active,source:'pointer',focus:true,requireActive:true,clearAnchor:!event.shiftKey||event.ctrlKey||event.metaKey});
    }
    ownerWindow(){return ListBox.windowOwner(this.document,this.view);}
    dispose(){if(this.disposed)return;this.disposed=true;this.onSelection=null;this.anchor=null;this.observer?.disconnect();for(const [node,type,handler] of this.listeners){if(node===this.view&&!this.ownerWindow())continue;node.removeEventListener(type,handler);}this.listeners=[];for(const [node,name,value] of this.attributes){if(value===null)node.removeAttribute(name);else node.setAttribute(name,value);}this.attributes=[];if(tableRoots.get(this.root)===this)tableRoots.delete(this.root);for(const node of this.known)if(tableMembers.get(node)===this)tableMembers.delete(node);}
  }
  const virtualRoots=new WeakMap();let virtualSequence=0;
  const virtualInteractive='button,input,select,textarea,a[href],area[href],summary,iframe,object,embed,audio[controls],video[controls],[contenteditable]:not([contenteditable="false"])';
  class VirtualList {
   constructor(root,items,{label,rowHeight=24,overscan=8,multiple=false,selected=[],active=0,onSelection=null}={}){
    const doc=root?.ownerDocument,view=doc?.defaultView,count=Array.isArray(items)?items.length:0;
    if(!view||view.closed||!(root instanceof view.HTMLElement)||!root.isConnected||root.isContentEditable||root.matches(virtualInteractive)||virtualRoots.has(root)||typeof label!=='string'||!label.trim()||label.length>256||!Array.isArray(items)||!Number.isInteger(count)||count<1||count>65536||!Number.isFinite(rowHeight)||rowHeight<12||rowHeight>256||count*rowHeight>1000000||!Number.isInteger(overscan)||overscan<0||overscan>32||typeof multiple!=='boolean'||(onSelection!==null&&typeof onSelection!=='function'))throw new TypeError('Invalid virtual list');
    const ids=new Set(),model=[];for(let index=0;index<count;index++){const item=items[index];if(typeof item?.id!=='string'||!item.id||item.id.length>128||ids.has(item.id)||typeof item.label!=='string'||!item.label.trim()||item.label.length>256||typeof item.disabled!=='boolean')throw new TypeError('Invalid virtual model');ids.add(item.id);model.push(Object.freeze({id:item.id,label:item.label,disabled:item.disabled}));}
    const initial=VirtualList.checked(selected,model.length,multiple);if(initial===null||!Number.isInteger(active)||active<0||active>=model.length)throw new TypeError('Invalid virtual selection');
    Object.assign(this,{root,model:Object.freeze(model),document:doc,view,label,rowHeight,overscan,multiple,onSelection,selected:initial,active,extent:model.length*rowHeight,disposed:false,busy:false,error:null,anchor:null,pressed:null,incoming:null,rows:new Map(),free:[],rowStyles:new WeakMap(),attributes:[],styles:[],listeners:[],spacer:null});
    if(!this.empty()||!this.live()||!this.coordinateProfile())throw new TypeError('Invalid virtual root profile');
    if(!Number.isSafeInteger(virtualSequence)||virtualSequence>=Number.MAX_SAFE_INTEGER)throw new RangeError('Virtual namespace exhausted');this.owner=virtualSequence+1;this.prefix='ravet-vl-'+this.owner+'-';
    // At most 65536 original-document lookups, before any presentation mutation.
    for(let index=0;index<model.length;index++)if(doc.getElementById(this.prefix+index))throw new TypeError('Virtual namespace collision');
    for(const name of ['role','aria-label','aria-labelledby','aria-multiselectable','aria-activedescendant','tabindex']){const value=root.getAttribute(name);if(value!==null&&value.length>4096)throw new TypeError('Virtual original attribute capacity');this.attributes.push([name,value]);}
    for(const name of ['position','display','overflow-x','overflow-y','padding','box-sizing','grid-template-columns','grid-template-rows','grid-template-areas','contain'])this.styles.push([name,root.style.getPropertyValue(name),root.style.getPropertyPriority(name)]);
    if((root.getAttribute('style')?.length??0)>16384||this.styles.some(([,value])=>value.length>4096))throw new TypeError('Virtual original style capacity');this.hadStyle=root.hasAttribute('style');virtualSequence=this.owner;virtualRoots.set(root,this);
    this.spacer=doc.createElement('div');this.spacer.style.cssText='position:relative;display:block;box-sizing:border-box;margin:0;padding:0;border:0;width:100%;height:'+this.extent+'px;pointer-events:none';this.spacerStyle=this.spacer.getAttribute('style');root.append(this.spacer);
    this.listen(root,'scroll',()=>this.refresh());this.listen(root,'keydown',event=>this.key(event));this.listen(root,'keyup',event=>{if(event.key==='Shift'||!event.shiftKey)this.anchor=null;});this.listen(doc,'pointerdown',event=>this.captureDown(event),true);this.listen(root,'pointerdown',event=>this.down(event));this.listen(doc,'pointerup',event=>this.up(event));this.listen(doc,'pointercancel',event=>{if(this.pressed?.pointer===event.pointerId)this.cancelPointer();});this.listen(root,'click',event=>this.click(event));this.listen(doc,'pointerdown',event=>{if(this.incoming?.event===event)this.incoming=null;if(!this.disposed&&!this.busy)this.refresh();});this.listen(view,'blur',()=>{this.anchor=null;this.cancelPointer();});this.listen(doc,'visibilitychange',()=>{if(doc.hidden){this.anchor=null;this.cancelPointer();}});
    this.resize=new view.ResizeObserver(()=>this.refresh());this.resize.observe(root);this.observer=new view.MutationObserver(()=>this.refresh());this.observer.observe(doc,{subtree:true,childList:true,attributes:true,attributeFilter:['id','hidden','inert','disabled','aria-hidden','aria-disabled','style','class','contenteditable','role','tabindex','aria-label','aria-labelledby','aria-selected','aria-activedescendant','aria-multiselectable','aria-posinset','aria-setsize']});this.refresh();
   }
   static checked(indices,count,multiple){if(!Array.isArray(indices))return null;const size=indices.length;if(!Number.isInteger(size)||size<0||size>256||(!multiple&&size>1))return null;const copied=[],seen=new Set();for(let at=0;at<size;at++){const index=indices[at];if(!Number.isInteger(index)||index<0||index>=count||seen.has(index))return null;seen.add(index);copied.push(index);}return copied.sort((a,b)=>a-b);}
   empty(){return this.root.childNodes.length<=64&&Array.from(this.root.childNodes).every(node=>node.nodeType===8||(node.nodeType===3&&/^[\t\n\f\r ]*$/.test(node.textContent)));}
   ownerWindow(){try{return this.document.defaultView===this.view&&this.view.document===this.document&&!this.view.closed;}catch{return false;}}
   live(){try{return this.document.defaultView===this.view&&this.view.document===this.document&&!this.view.closed&&(!this.view.frameElement||this.view.frameElement.isConnected)&&this.root.isConnected&&this.root.ownerDocument===this.document&&!this.root.isContentEditable&&!this.root.matches(virtualInteractive);}catch{return false;}}
   plainStyle(style){return style.transform==='none'&&['translate','rotate','scale','perspective'].every(name=>style[name]===undefined||style[name]===''||style[name]==='none')&&(style.zoom===undefined||style.zoom===''||style.zoom==='normal'||Number(style.zoom)===1);}
   pseudoProfile(node,root=false){for(const pseudo of ['::before','::after']){const style=this.view.getComputedStyle(node,pseudo);if(!['normal','none'].includes(style.content)&&style.display!=='none'&&(!root||!['absolute','fixed'].includes(style.position)))return false;}return true;}
   coordinateProfile(){try{let node=this.root,count=0;for(;node;node=node.parentElement){if(++count>64)return false;const style=this.view.getComputedStyle(node);if(!this.plainStyle(style))return false;}return this.pseudoProfile(this.root,true);}catch{return false;}}
   valid(){if(!this.live()||!this.spacer||this.spacer.parentNode!==this.root||this.spacer.getAttribute('style')!==this.spacerStyle||this.root.childNodes.length>65)return false;for(const node of this.root.childNodes)if(node!==this.spacer&&!(node.nodeType===8||(node.nodeType===3&&/^[\t\n\f\r ]*$/.test(node.textContent))))return false;if(this.rows.size+this.free.length>512||this.free.some(row=>row.parentNode!==null||row.ownerDocument!==this.document||row.localName!=='div'||row.getAttribute('style')!==this.rowStyles.get(row)))return false;if(this.spacer.children.length!==this.rows.size||this.spacer.childNodes.length!==this.rows.size)return false;for(const [index,row] of this.rows){if(row.parentNode!==this.spacer||row.ownerDocument!==this.document||row.localName!=='div'||row.id!==this.prefix+index||this.document.getElementById(row.id)!==row||row.childNodes.length!==1||row.firstChild.nodeType!==3||row.textContent!==this.model[index].label||row.isContentEditable||['hidden','inert','disabled','aria-hidden','contenteditable'].some(name=>row.hasAttribute(name))||row.getAttribute('style')!==this.rowStyles.get(row))return false;}return true;}
   listen(node,type,handler,capture=false){node.addEventListener(type,handler,capture);this.listeners.push([node,type,handler,capture]);}
   write(node,name,value){if(this.disposed)return false;if(node.getAttribute(name)!==value){if(value===null)node.removeAttribute(name);else node.setAttribute(name,value);}return !this.disposed;}
   profile(node,pairs){for(const [name,value] of pairs)if(!this.write(node,name,value))return false;return true;}
   css(node,name,value){if(this.disposed)return false;if(node.style.getPropertyValue(name)!==value||node.style.getPropertyPriority(name)!=='important')node.style.setProperty(name,value,'important');return !this.disposed;}
   rootReady(){return !this.root.closest('[hidden],[inert],[aria-hidden="true"],[aria-disabled="true"],[disabled]')&&this.view.getComputedStyle(this.root).visibility==='visible'&&this.root.getClientRects().length>0;}
   eligible(index){return Number.isInteger(index)&&index>=0&&index<this.model.length&&!this.model[index].disabled;}
   fallback(){if(this.eligible(this.active))return;for(let distance=0;distance<this.model.length;distance++){if(this.eligible(this.active+distance)){this.active+=distance;return;}if(this.eligible(this.active-distance)){this.active-=distance;return;}}this.active=-1;}
   suspend(code){this.error=code;this.cancelPointer(false);if(!this.profile(this.root,[['tabindex','-1'],['aria-activedescendant',null]]))return null;return {ready:false,code};}
   sync(){
    if(this.disposed)return null;if(!this.valid()){this.error='Ownership';this.dispose();return null;}
    if(!this.profile(this.root,[['role','listbox'],['aria-label',this.label],['aria-labelledby',null],['aria-multiselectable',this.multiple?'true':'false']]))return null;
    for(const [name,value] of [['position','relative'],['display',this.root.hidden?'none':'block'],['overflow-x','hidden'],['overflow-y','auto'],['padding','0px'],['box-sizing','border-box'],['grid-template-columns','none'],['grid-template-rows','none'],['grid-template-areas','none'],['contain','none']])if(!this.css(this.root,name,value))return null;
    if(!this.valid()){this.error='Ownership';this.dispose();return null;}if(!this.coordinateProfile()||!this.plainStyle(this.view.getComputedStyle(this.spacer))||!this.pseudoProfile(this.spacer))return this.suspend('Coordinates');
    const height=this.root.clientHeight,width=this.root.clientWidth,scroll=this.root.scrollTop;if(!Number.isFinite(height)||!Number.isFinite(width))return this.suspend('Geometry');if(!this.rootReady()||height<=0||this.root.clientWidth<=0)return this.suspend('Unavailable');if(height>4096)return this.suspend('TooLarge');if(!Number.isFinite(scroll)||scroll<0||scroll>Math.max(0,this.extent-height)+2||Math.abs(this.root.scrollHeight-Math.max(this.extent,height))>2)return this.suspend('Geometry');
    this.fallback();if(this.active<0)return this.suspend('Unavailable');
    const first=Math.max(0,Math.floor(scroll/this.rowHeight)-this.overscan),last=Math.min(this.model.length-1,Math.ceil((scroll+height)/this.rowHeight)-1+this.overscan),desired=new Set();for(let index=first;index<=last;index++)desired.add(index);desired.add(this.active);if(this.pressed)desired.add(this.pressed.index);if(desired.size>512)return this.suspend('Capacity');
    for(const [index,row] of this.rows)if(!desired.has(index)){row.remove();this.rows.delete(index);this.free.push(row);if(this.disposed)return null;}
    for(const index of desired){let row=this.rows.get(index);if(!row){if(this.document.getElementById(this.prefix+index)){this.error='Collision';this.dispose();return null;}row=this.free.pop()??this.document.createElement('div');row.id=this.prefix+index;if(this.disposed)return null;row.textContent=this.model[index].label;if(this.disposed)return null;row.style.cssText='position:absolute!important;display:block!important;left:0!important;right:0!important;top:'+(index*this.rowHeight)+'px!important;height:'+this.rowHeight+'px!important;box-sizing:border-box!important;margin:0!important;padding:0!important;border:0!important;overflow:hidden!important;white-space:nowrap!important;pointer-events:auto!important;';if(this.disposed)return null;this.rowStyles.set(row,row.getAttribute('style'));if(this.disposed)return null;this.rows.set(index,row);this.spacer.append(row);if(this.disposed)return null;}
     if(!this.profile(row,[['role','option'],['aria-label',this.model[index].label],['aria-labelledby',null],['aria-selected',String(this.selected.includes(index))],['aria-disabled',String(this.model[index].disabled)],['aria-setsize',String(this.model.length)],['aria-posinset',String(index+1)],['tabindex','-1']]))return null;
     const style=this.view.getComputedStyle(row),rect=row.getBoundingClientRect();if(!this.plainStyle(style)||!this.pseudoProfile(row))return this.suspend('Coordinates');if(![rect.top,rect.left,rect.right,rect.bottom,rect.height,rect.width,row.offsetTop].every(Number.isFinite)||Math.abs(rect.width-width)>2||style.transform!=='none'||style.position!=='absolute'||Math.abs(rect.height-this.rowHeight)>2||Math.abs(row.offsetTop-index*this.rowHeight)>2)return this.suspend('Geometry');
    }
    if(!this.profile(this.root,[['tabindex','0'],['aria-activedescendant',this.prefix+this.active]]))return null;if(!this.valid()){this.error='Ownership';this.dispose();return null;}if(!this.pressed?.consumed)this.error=null;return {ready:true,height,scroll};
   }
   refresh(){if(this.disposed||this.busy)return false;this.busy=true;try{return this.sync()?.ready===true;}finally{this.busy=false;}}
   activeIndex(){return this.active;}
   renderedIndices(){return Object.freeze([...this.rows.keys()].sort((a,b)=>a-b));}
   snapshot(source=null){return Object.freeze({selectedIndices:Object.freeze(this.selected.slice()),selectedIds:Object.freeze(this.selected.map(index=>this.model[index].id)),unavailableSelectedIds:Object.freeze(this.selected.filter(index=>this.model[index].disabled).map(index=>this.model[index].id)),activeIndex:this.active,activeId:this.active<0?null:this.model[this.active].id,...(source?{source}:{})});}
   selection(){return this.disposed||!this.valid()?null:this.snapshot();}
   lastError(){return this.error;}
   setSelected(indices,{notify=false}={}){if(this.disposed||this.busy||typeof notify!=='boolean')return false;const selected=VirtualList.checked(indices,this.model.length,this.multiple);if(selected===null){this.error='Capacity';return false;}return this.commit({selected,source:notify?'programmatic':null,clearAnchor:true});}
   setActive(index,{focus=false,scroll=true}={}){if(this.disposed||this.busy||typeof focus!=='boolean'||typeof scroll!=='boolean'||!this.eligible(index))return false;return this.commit({active:index,focus,scroll,clearAnchor:true});}
   commit({selected=this.selected.slice(),active=this.active,source=null,focus=false,scroll=false,clearAnchor=false,anchor=this.anchor}={}){
    if(this.disposed||this.busy)return false;if(VirtualList.checked(selected,this.model.length,this.multiple)===null){this.error='Capacity';return false;}if(!this.valid()){this.error='Ownership';this.dispose();return false;}if(!(this.eligible(active)||(active===-1&&!this.model.some(item=>!item.disabled)))||selected.some(index=>!this.selected.includes(index)&&!this.eligible(index)))return false;const changed=selected.length!==this.selected.length||selected.some((index,at)=>index!==this.selected[at]);this.busy=true;let phase='Lifecycle';
    try{if(focus){phase='Focus';this.root.focus({preventScroll:true});if(this.disposed)return false;if(!this.valid()){this.error='Ownership';this.dispose();return false;}if(this.document.activeElement!==this.root||!this.rootReady()||!this.coordinateProfile()){this.error='Focus';return false;}}
     phase='Lifecycle';const before=this.sync();if(!before||(!before.ready&&(focus||source==='keyboard'||source==='pointer')))return false;if(source!=='pointer')this.cancelPointer(false);this.selected=selected.slice();this.active=active;this.anchor=clearAnchor?null:anchor;
     if(scroll){const top=active*this.rowHeight,bottom=top+this.rowHeight;if(top<this.root.scrollTop)this.root.scrollTop=top;else if(bottom>this.root.scrollTop+this.root.clientHeight)this.root.scrollTop=bottom-this.root.clientHeight;}
     const rendered=this.sync();if(!rendered)return false;if(changed&&source&&this.onSelection){phase='Callback';this.onSelection(this.snapshot(source));phase='Lifecycle';}if(this.disposed)return false;const final=this.sync();if(!final||(!final.ready&&before.ready))return false;if((focus||source==='keyboard'||source==='pointer')&&this.document.activeElement!==this.root){this.error='Focus';return false;}return true;
    }catch{if(!this.disposed){try{if(!this.valid())this.dispose();else this.sync();}catch{if(!this.disposed)this.dispose();}}this.error=phase;return false;}finally{this.busy=false;}
   }
   range(target){const anchor=this.anchor??{index:this.active,baseline:this.selected.slice()},selected=new Set(anchor.baseline),low=Math.min(anchor.index,target),high=Math.max(anchor.index,target);for(let index=low;index<=high;index++)if(this.eligible(index)){selected.add(index);if(selected.size>256)return null;}return {selected:[...selected].sort((a,b)=>a-b),anchor};}
   key(event){if(event.defaultPrevented||event.isComposing||event.altKey||this.disposed||this.busy||this.document.activeElement!==this.root||!['ArrowUp','ArrowDown','Home','End',' '].includes(event.key)||(event.key===' '&&(event.ctrlKey||event.metaKey))||!this.refresh())return;let target=this.active;if(event.key==='Home'){for(let index=0;index<this.model.length;index++)if(this.eligible(index)){target=index;break;}}else if(event.key==='End'){for(let index=this.model.length-1;index>=0;index--)if(this.eligible(index)){target=index;break;}}else if(event.key==='ArrowUp'||event.key==='ArrowDown'){const step=event.key==='ArrowUp'?-1:1;for(let index=target+step;index>=0&&index<this.model.length;index+=step)if(this.eligible(index)){target=index;break;}}
    const range=this.multiple&&event.shiftKey&&!event.ctrlKey&&!event.metaKey&&event.key!==' ';event.preventDefault();if(event.key!==' '&&target===this.active){this.cancelPointer();if(!range)this.anchor=null;return;}let selected=this.selected.slice(),anchor=this.anchor;if(this.multiple){if(event.key===' ')selected=selected.includes(target)?selected.filter(index=>index!==target):[...selected,target].sort((a,b)=>a-b);else if(range){const value=this.range(target);if(!value){this.error='Capacity';return;}selected=value.selected;anchor=value.anchor;}}else if(!event.ctrlKey&&!event.metaKey)selected=[target];this.commit({selected,active:target,source:'keyboard',scroll:true,clearAnchor:!range,anchor});
   }
   rowAt(event){let node=event.target;if(!(node instanceof this.view.Element))node=node?.parentElement;for(;node&&node!==this.root;node=node.parentElement)for(const [index,row] of this.rows)if(row===node)return {index,row};return null;}
   captureDown(event){if(this.disposed)return;this.incoming=null;const hit=this.root.contains(event.target)?this.rowAt(event):null;this.cancelPointer(false);if(hit)this.incoming={event,...hit,id:this.model[hit.index].id,owner:this.owner};}
   down(event){const candidate=this.incoming?.event===event?this.incoming:null;this.incoming=null;if(event.defaultPrevented||!event.isTrusted||event.button!==0||event.altKey||this.disposed||this.busy||!candidate)return;if(!this.refresh()||this.rows.get(candidate.index)!==candidate.row||this.rowAt(event)?.row!==candidate.row||candidate.owner!==this.owner||candidate.id!==this.model[candidate.index].id||!this.eligible(candidate.index))return;this.pressed={row:candidate.row,index:candidate.index,id:candidate.id,pointer:event.pointerId,up:false,consumed:false,owner:this.owner};}
   up(event){const token=this.pressed;if(!token||token.consumed||event.pointerId!==token.pointer||!event.isTrusted)return;const hit=this.rowAt(event);if(event.button!==0||!hit||hit.row!==token.row||hit.index!==token.index){this.cancelPointer();return;}token.up=true;}
   cancelPointer(render=true){if(!this.pressed)return;this.pressed=null;if(render&&!this.disposed&&!this.busy)this.refresh();}
   click(event){const token=this.pressed;if(!token||!token.up||token.consumed||!event.isTrusted||!(event instanceof this.view.PointerEvent)||event.pointerId!==token.pointer||event.button!==0||event.altKey||event.defaultPrevented||this.disposed||this.busy)return;const hit=this.rowAt(event);if(!hit||hit.row!==token.row||hit.index!==token.index||token.owner!==this.owner||this.model[token.index].id!==token.id||!this.refresh()||!this.eligible(hit.index))return;const rect=hit.row.getBoundingClientRect(),root=this.root.getBoundingClientRect();if(!Number.isFinite(event.clientX)||!Number.isFinite(event.clientY)||event.clientY<Math.max(rect.top,root.top)||event.clientY>=Math.min(rect.bottom,root.bottom)||event.clientX<Math.max(rect.left,root.left)||event.clientX>=Math.min(rect.right,root.right))return;let selected=[hit.index],anchor=this.anchor;const range=this.multiple&&event.shiftKey&&!event.ctrlKey&&!event.metaKey;if(this.multiple){if(range){const value=this.range(hit.index);if(!value){token.consumed=true;this.error='Capacity';return;}selected=value.selected;anchor=value.anchor;}else if(event.ctrlKey||event.metaKey)selected=this.selected.includes(hit.index)?this.selected.filter(index=>index!==hit.index):[...this.selected,hit.index].sort((a,b)=>a-b);}if(VirtualList.checked(selected,this.model.length,this.multiple)===null){token.consumed=true;this.error='Capacity';return;}token.consumed=true;event.preventDefault();const accepted=this.commit({selected,active:hit.index,focus:true,source:'pointer',clearAnchor:!range,anchor});if(this.pressed===token){const failure=this.error;this.pressed=null;if(!this.disposed&&!this.busy)this.refresh();if(!accepted)this.error=failure;}return accepted;}
   dispose(){if(this.disposed)return;this.disposed=true;this.onSelection=null;this.anchor=null;this.pressed=null;this.incoming=null;this.resize?.disconnect();this.observer?.disconnect();for(const [node,type,handler,capture] of this.listeners){if(node===this.view&&!this.ownerWindow())continue;node.removeEventListener(type,handler,capture);}this.listeners=[];this.spacer?.remove();this.rows.clear();this.free=[];for(const [name,value] of this.attributes){if(value===null)this.root.removeAttribute(name);else this.root.setAttribute(name,value);}this.attributes=[];for(const [name,value,priority] of this.styles){if(value)this.root.style.setProperty(name,value,priority);else this.root.style.removeProperty(name);}this.styles=[];if(!this.hadStyle&&this.root.style.length===0&&this.root.getAttribute('style')!==null)this.root.removeAttribute('style');if(virtualRoots.get(this.root)===this)virtualRoots.delete(this.root);}
  }
  window.RavetControls=Object.freeze({Slider,Knob,PianoKeyboard,Toggle,Choice,ActionButton,AccessibleMeter,RovingFocus,TextEditor,TextEditorError,PopupMenu,TabbedPanels,ModalDialog,SplitPane,ListBox,TreeView,TableView,VirtualList});
})();
