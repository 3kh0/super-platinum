const onKeypress = () => dioxus.send("keypress");
document.addEventListener("keypress", onKeypress, {passive: true});
try {
  dioxus.send("ready");
  await dioxus.recv();
} finally {
  document.removeEventListener("keypress", onKeypress);
}
return null;
