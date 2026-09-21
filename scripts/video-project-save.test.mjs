import test from 'node:test';
import assert from 'node:assert/strict';
import {createVideoProjectSaveQueue,sameVideoProjectValue} from '../src/windows/video-project-save.js';
import {installVideoProjectShortcuts} from '../src/windows/video-project-shortcuts.js';

const deferred=()=>{let resolve,reject;const promise=new Promise((a,b)=>{resolve=a;reject=b;});return{promise,resolve,reject};};
const tick=()=>new Promise(resolve=>setImmediate(resolve));
const result=revision=>({state:'valid',revision});
function fixture(t,options={}){
  const calls=[];
  const queue=createVideoProjectSaveQueue({revision:'base',project:{edit:'original'},delay:60_000,
    save:(revision,project)=>{const pending=deferred();calls.push({revision,project,...pending});return pending.promise;},...options});
  t.after(()=>queue.dispose());return{queue,calls};
}

test('closing an untouched edit does not write an empty project',async t=>{
  const {queue,calls}=fixture(t,{project:null});
  assert.equal(await queue.flush(),true);assert.equal(calls.length,0);
});

test('cancelling a first text draft during debounce leaves no empty project',async t=>{
  const {queue,calls}=fixture(t,{project:null});queue.enqueue({edit:'unfinished text'});
  assert.equal(queue.discardUnwritten(),true);assert.equal(await queue.flush(),true);
  assert.equal(calls.length,0);assert.equal(queue.getState().dirty,false);
});

test('cancelling a draft after its write starts persists the reverted edit',async t=>{
  const {queue,calls}=fixture(t,{project:null});queue.enqueue({edit:'unfinished text'});const pending=queue.flush();
  assert.equal(queue.discardUnwritten(),false);queue.enqueue({edit:'original'});
  calls[0].resolve(result('draft-written'));await tick();
  assert.equal(calls[1].revision,'draft-written');assert.deepEqual(calls[1].project,{edit:'original'});
  calls[1].resolve(result('reverted'));assert.equal(await pending,true);
});

test('slow saves serialize and coalesce to the latest edit using the returned revision',async t=>{
  const {queue,calls}=fixture(t);
  queue.enqueue({edit:'first'});const closing=queue.flush();
  queue.enqueue({edit:'intermediate'});queue.enqueue({edit:'latest'});
  assert.equal(calls.length,1);
  calls[0].resolve(result('revision-1'));await tick();
  assert.equal(calls.length,2);
  assert.deepEqual([calls[1].revision,calls[1].project],['revision-1',{edit:'latest'}]);
  calls[1].resolve(result('revision-2'));
  assert.equal(await closing,true);assert.equal(queue.getState().dirty,false);
  assert.equal(queue.getState().revision,'revision-2');
});

test('failed saving retains the newest pending edit and close reports failure until retry succeeds',async t=>{
  const {queue,calls}=fixture(t);
  queue.enqueue({edit:'first'});const closing=queue.flush();
  queue.enqueue({edit:'latest'});calls[0].reject('VIDEO_PROJECT_IO');
  assert.equal(await closing,false);
  assert.equal(queue.getState().dirty,true);assert.equal(queue.getState().status,'error');
  assert.equal(calls.length,1);
  const retry=queue.flush();
  assert.deepEqual([calls[1].revision,calls[1].project],['base',{edit:'latest'}]);
  calls[1].resolve(result('retry-revision'));
  assert.equal(await retry,true);assert.equal(queue.getState().dirty,false);
});

test('undo during a write restores the saved baseline after that write completes',async t=>{
  const {queue,calls}=fixture(t);
  queue.enqueue({edit:'changed'});const closing=queue.flush();
  queue.enqueue({edit:'original'});calls[0].resolve(result('changed-revision'));await tick();
  assert.equal(calls.length,2);assert.equal(calls[1].revision,'changed-revision');
  assert.deepEqual(calls[1].project,{edit:'original'});calls[1].resolve(result('restored-revision'));
  assert.equal(await closing,true);
});

test('close includes an edit queued at the exact save completion boundary',async t=>{
  let added=false,queue;
  const setup=fixture(t,{onState:state=>{
    if(state.status==='saved'&&!added){added=true;queue.enqueue({edit:'completion-boundary'});}
  }});
  queue=setup.queue;
  queue.enqueue({edit:'first'});const closing=queue.flush();
  setup.calls[0].resolve(result('first'));await tick();
  assert.equal(setup.calls.length,2);
  assert.deepEqual(setup.calls[1].project,{edit:'completion-boundary'});
  setup.calls[1].resolve(result('last'));assert.equal(await closing,true);
  assert.equal(queue.getState().dirty,false);
});

test('a revision conflict is never silently rebased or retried over somebody else’s edit',async t=>{
  const {queue,calls}=fixture(t);
  queue.enqueue({edit:'mine'});const pending=queue.flush();
  calls[0].reject('VIDEO_PROJECT_CONFLICT');assert.equal(await pending,false);
  const retry=queue.flush();assert.equal(calls[1].revision,'base');
  calls[1].reject('VIDEO_PROJECT_CONFLICT');assert.equal(await retry,false);
  assert.equal(queue.getState().dirty,true);
});

test('equivalent snapshots do not save or stringify embedded image data',async t=>{
  const image='data:image/png;base64,'+'A'.repeat(1024*1024);
  const original={edit:{stickers:[{id:'image',dataUrl:image}],segments:[{start:0,end:1}]}};
  const copy={...original,edit:{...original.edit,stickers:[{...original.edit.stickers[0]}]}};
  assert.equal(sameVideoProjectValue(original,copy),true);
  const {queue,calls}=fixture(t,{project:original});queue.enqueue(copy);
  assert.equal(await queue.flush(),true);assert.equal(calls.length,0);
});

test('debounced updates write once, and disposal cancels a pending write',async t=>{
  const calls=[];
  const queue=createVideoProjectSaveQueue({revision:'base',delay:10,save:async(revision,project)=>{calls.push(project);return result('saved');}});
  t.after(()=>queue.dispose());
  queue.enqueue({edit:'a'});queue.enqueue({edit:'b'});queue.enqueue({edit:'c'});
  await new Promise(resolve=>setTimeout(resolve,30));assert.deepEqual(calls,[{edit:'c'}]);
  queue.enqueue({edit:'disposed'});queue.dispose();
  await new Promise(resolve=>setTimeout(resolve,30));assert.equal(calls.length,1);
});

// An inline textarea stops bubbling after its own keyboard handler. Keep this
// phase-aware harness small: a bubble-only regression must miss the shortcut.
function textEditorSurface(){
  const listeners=[];
  return{
    addEventListener(type,callback,capture){listeners.push({type,callback,capture});},
    removeEventListener(type,callback,capture){const index=listeners.findIndex(item=>item.type===type&&item.callback===callback&&item.capture===capture);if(index>=0)listeners.splice(index,1);},
    key(options){
      const event={key:'',metaKey:false,ctrlKey:false,altKey:false,shiftKey:false,isComposing:false,repeat:false,keyCode:0,defaultPrevented:false,cancelBubble:false,
        preventDefault(){this.defaultPrevented=true;},stopPropagation(){this.cancelBubble=true;},...options};
      for(const item of listeners.filter(item=>item.capture===true))item.callback(event);
      const reachedTextEditor=!event.cancelBubble;
      if(reachedTextEditor)event.stopPropagation();
      if(!event.cancelBubble)for(const item of listeners.filter(item=>item.capture!==true))item.callback(event);
      return{event,reachedTextEditor};
    },
  };
}

test('Cmd/Ctrl+S commits the current text before flushing, despite a textarea consuming bubble events',async t=>{
  for(const modifier of ['metaKey','ctrlKey']){
    const {queue,calls}=fixture(t),surface=textEditorSurface();
    let saving;
    const stop=installVideoProjectShortcuts(surface,{save:()=>{queue.enqueue({edit:'刚输入而未点完成的文字'});saving=queue.flush();}});
    t.after(stop);
    const handled=surface.key({key:'s',[modifier]:true});
    assert.equal(handled.reachedTextEditor,false);assert.equal(handled.event.defaultPrevented,true);
    assert.equal(calls.length,1);assert.deepEqual(calls[0].project,{edit:'刚输入而未点完成的文字'});
    calls[0].resolve(result('text-saved'));assert.equal(await saving,true);
  }
});

test('Ctrl+W from the text field goes through the close flush and waits for the latest edit',async t=>{
  const {queue,calls}=fixture(t),surface=textEditorSurface();let closed=0,closing;
  t.after(installVideoProjectShortcuts(surface,{close:()=>{
    queue.enqueue({edit:'latest inline text'});
    closing=queue.flush().then(saved=>{if(saved)closed++;});
  }}));
  const handled=surface.key({key:'w',ctrlKey:true});
  assert.equal(handled.reachedTextEditor,false);assert.equal(closed,0);
  assert.deepEqual(calls[0].project,{edit:'latest inline text'});
  calls[0].resolve(result('closed-text'));await closing;assert.equal(closed,1);
});

test('video capture shortcuts leave Escape, native undo, IME and modified variants with the text editor',t=>{
  const surface=textEditorSurface();let invoked=0;
  t.after(installVideoProjectShortcuts(surface,{save:()=>invoked++,close:()=>invoked++}));
  for(const options of [
    {key:'Escape'},{key:'z',metaKey:true},{key:'z',ctrlKey:true},{key:'z',metaKey:true,shiftKey:true},
    {key:'s'},{key:'s',ctrlKey:true,isComposing:true},{key:'w',metaKey:true,isComposing:true},
    {key:'s',ctrlKey:true,keyCode:229},{key:'s',ctrlKey:true,altKey:true},{key:'w',metaKey:true,shiftKey:true},
  ]){
    const outcome=surface.key(options);assert.equal(outcome.reachedTextEditor,true);
    assert.equal(outcome.event.defaultPrevented,false);
  }
  assert.equal(invoked,0);
});

test('video shortcut cleanup and repeat handling avoid duplicate writes or close requests',()=>{
  const surface=textEditorSurface();let saved=0,closed=0;
  const stopClose=installVideoProjectShortcuts(surface,{close:()=>closed++});
  const stopSave=installVideoProjectShortcuts(surface,{save:()=>saved++});
  surface.key({key:'s',metaKey:true});surface.key({key:'s',metaKey:true,repeat:true});
  surface.key({key:'w',ctrlKey:true});surface.key({key:'w',ctrlKey:true,repeat:true});
  assert.equal(saved,1);assert.equal(closed,1);
  stopClose();stopSave();
  assert.equal(surface.key({key:'s',metaKey:true}).reachedTextEditor,true);
  assert.equal(surface.key({key:'w',ctrlKey:true}).reachedTextEditor,true);
  assert.equal(saved,1);assert.equal(closed,1);
});
