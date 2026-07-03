export function installDesktopWebViewGuards() {
  document.addEventListener("contextmenu", (event) => {
    event.preventDefault();
  });

  window.addEventListener("keydown", (event) => {
    const key = event.key.toLowerCase();
    const refreshShortcut =
      event.key === "F5" || ((event.ctrlKey || event.metaKey) && key === "r");
    const navigationShortcut =
      event.altKey && (event.key === "ArrowLeft" || event.key === "ArrowRight");

    if (refreshShortcut || navigationShortcut) {
      event.preventDefault();
      event.stopPropagation();
    }
  });
}
