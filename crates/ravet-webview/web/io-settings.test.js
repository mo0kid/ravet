// Device-free tests of the actual popup script, using a small DOM/bridge fake.
// Native WebKit rendering and interaction remain a separate integration gate.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const html = fs.readFileSync(`${__dirname}/io-settings.html`, 'utf8');
class Node {
 constructor(tag='div'){this.tag=tag;this.children=[];this.options=[];this.dataset={};this.hidden=false;this.disabled=false;this.open=false;this._value='';this.listeners={};}
 get value(){return this._value;}
 set value(value){this._value=String(value);if(this.tag==='select'&&!this.options.some(o=>o.value===this._value))this._value='';}
 replaceChildren(){this.children=[];this.options=[];this._value='';}
 append(...nodes){this.children.push(...nodes);}
 add(option){this.options.push(option);if(this.options.length===1)this._value=option.value;}
 querySelectorAll(selector){const all=this.children.flatMap(n=>[n,...(n.querySelectorAll?.('input')||[])]);return all.filter(n=>selector==='input:checked'?n.tag==='input'&&n.checked:n.tag==='input'||selector==='select,input'&&n.tag==='select');}
 addEventListener(type,fn){this.listeners[type]=fn;}
 change(){this.onchange?.();this.listeners.change?.();}
 click(){if(!this.disabled)this.onclick?.();}
 showModal(){this.open=true;}
 close(){this.open=false;}
}
function fixture(saved){
 const ids=new Map([...html.matchAll(/id="([^"]+)"/g)].map(m=>[m[1],new Node(html.includes(`<select id="${m[1]}"`)?'select':'div')]));
 const actions=[],preferences=[];
 const window={ravetSavedSettings:saved,webkit:{messageHandlers:{ravet:{postMessage:m=>actions.push(m)},ravetSettings:{postMessage:m=>preferences.push(m)}}}};
 const document={getElementById:id=>ids.get(id),createElement:tag=>new Node(tag),createTextNode:text=>({text})};
 ids.get('io-settings').querySelectorAll=selector=>[...ids.values()].flatMap(n=>n.tag==='select'?[n]:Node.prototype.querySelectorAll.call(n,selector));
 const Option=function(name,value){this.name=name;this.value=String(value);};
 vm.runInNewContext(html.match(/<script>([\s\S]*)<\/script>/)[1],{window,document,Option});
 const io={outputs:[
  {id:'a',name:'A',channels:4,rates:[44100,48000,96000],default_rate:48000,nominal_rate:96000,buffers:[128,256]},
  {id:'b',name:'B',channels:2,rates:[44100,48000,96000],default_rate:48000,nominal_rate:44100,buffers:[128]},
  {id:'legacy',name:'Legacy',channels:2,rates:[44100,48000],default_rate:48000,buffers:[]}
 ],inputs:[{id:'input',name:'Input',channels:2}],midis:[]};
 const get=id=>ids.get(id);
 const choose=(id,value)=>{get(id).value=value;get(id).change();};
 const update=extra=>window.ravetUpdateSettings({io,running:false,status:'Ready',...extra});
 update();return {get,choose,update,window,actions,preferences};
}
const f=fixture();
for(const [device,rate] of [['a','96000'],['b','44100'],['legacy','48000'],['a','96000']]){
 f.choose('audio-output',device);assert.equal(f.get('sample-rate').value,rate);
}
f.choose('audio-input','input');f.choose('sample-rate','48000');f.window.ravetOpenSettings();
const mismatch={direction:'output',device:'a',requested:48000,actual:96000};
f.update({sample_rate_mismatch:mismatch});
assert.match(f.get('settings-status').textContent,/96000 Hz.*48000 Hz/);
assert.equal(f.get('use-actual-rate').hidden,false);
f.get('use-actual-rate').click();assert.equal(f.get('sample-rate').value,'96000');
assert.equal(f.preferences.at(-1).sample_rate,96000);assert.equal(f.actions.length,0);
f.update({sample_rate_mismatch:mismatch});assert.equal(f.get('use-actual-rate').hidden,true);
f.update({sample_rate_mismatch:{...mismatch,requested:96000}});
assert.equal(f.get('use-actual-rate').hidden,false); // Equal-rate timeout still reported.
f.update({sample_rate_mismatch:{...mismatch,actual:192000}});
assert.equal(f.get('use-actual-rate').disabled,true);
f.get('use-actual-rate').click();assert.equal(f.get('sample-rate').value,'96000');
f.update({sample_rate_mismatch:{direction:'input',device:'input',requested:48000,actual:44100}});
assert.match(f.get('settings-status').textContent,/44100 Hz/);
assert.equal(f.get('use-actual-rate').hidden,true);assert.equal(f.get('sample-rate').value,'96000');
f.update({sample_rate_mismatch:{...mismatch,device:'missing'}});assert.equal(f.get('settings-status').textContent,'Ready');
f.update({running:true,sample_rate_mismatch:mismatch});assert.equal(f.get('use-actual-rate').hidden,true);assert.equal(f.get('sample-rate').disabled,true);
f.update();f.window.ravetStartAudio();assert.equal(f.actions.at(-1).sample_rate,96000);
const restored=fixture({version:1,output:'a',input:'',sample_rate:48000,buffer_size:256,output_pairs:[2],input_pairs:[],midis:[]});
assert.equal(restored.get('sample-rate').value,'48000');assert.equal(restored.get('buffer-size').value,'256');
assert.equal(restored.actions.length,0); // Restore never starts audio.
console.log('PASS popup nominal defaults, saved selection, mismatch recovery and explicit Start (DOM fake)');
