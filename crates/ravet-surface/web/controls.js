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
  window.RavetControls=Object.freeze({Slider,Knob,PianoKeyboard,Toggle,Choice,ActionButton,AccessibleMeter,RovingFocus,TextEditor,TextEditorError,PopupMenu,TabbedPanels,ModalDialog});
})();
