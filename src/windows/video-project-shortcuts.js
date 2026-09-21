/** Video-only shortcuts must run before the inline text editor stops bubbling. */
export function installVideoProjectShortcuts(target,actions) {
  const onKey=event=>{
    if(event.defaultPrevented||event.isComposing||event.keyCode===229||
      (!event.metaKey&&!event.ctrlKey)||event.altKey||event.shiftKey)return;
    const key=event.key.toLowerCase();
    const action=key==="s"?actions.save:key==="w"?actions.close:undefined;
    if(!action)return;
    event.preventDefault();event.stopPropagation();
    if(!event.repeat)action();
  };
  target.addEventListener("keydown",onKey,true);
  return()=>target.removeEventListener("keydown",onKey,true);
}
