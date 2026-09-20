if (window.kayFullscreenHandler) {
  window.removeEventListener('keydown', window.kayFullscreenHandler, true);
}
window.kayFullscreenHandler = event => {
  if (event.code !== 'F11' || event.isComposing ||
      event.altKey || event.metaKey || event.ctrlKey || event.shiftKey) return;
  event.preventDefault();
  event.stopImmediatePropagation();
  if (!event.repeat) dioxus.send('fullscreen');
};
window.addEventListener('keydown', window.kayFullscreenHandler, true);
await new Promise(() => {});
