/**
 * Title bar (REQ-DCLEAN-032). The traffic lights are the native macOS ones
 * (titleBarStyle "Overlay" in tauri.conf.json), positioned over the reserved
 * `.titlebar-lights` slot where the snapshot draws its dots. The whole bar
 * drags the window.
 */
export function TitleBar() {
  return (
    <header class="titlebar" data-tauri-drag-region data-testid="titlebar">
      <span class="titlebar-lights" data-tauri-drag-region data-testid="traffic-lights" aria-hidden="true" />
      <div class="titlebar-title" data-tauri-drag-region>
        disky
      </div>
    </header>
  );
}
