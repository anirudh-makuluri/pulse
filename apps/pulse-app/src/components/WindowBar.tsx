import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";

const isDevelopment = import.meta.env.DEV;

export function WindowBar() {
  const desktopWindow = getCurrentWebviewWindow();

  return (
    <header
      className="window-bar"
      data-tauri-drag-region
      onMouseDown={(event) => {
        if (event.button === 0) void desktopWindow.startDragging();
      }}
      onDoubleClick={() => void desktopWindow.toggleMaximize()}
    >
      <div className="window-identity" data-tauri-drag-region>
        <img className="window-mark" src="/pulse-logo.png" alt="" aria-hidden="true" />
        <span>Pulse</span>
        {isDevelopment ? <span className="environment-label">( dev )</span> : null}
      </div>
      <div className="window-controls">
        <button
          className="window-control minimize"
          type="button"
          aria-label="Minimize window"
          onMouseDown={(event) => event.stopPropagation()}
          onClick={() => void desktopWindow.minimize()}
        />
        <button
          className="window-control maximize"
          type="button"
          aria-label="Maximize or restore window"
          onMouseDown={(event) => event.stopPropagation()}
          onClick={() => void desktopWindow.toggleMaximize()}
        />
        <button
          className="window-control close"
          type="button"
          aria-label="Hide Pulse window"
          onMouseDown={(event) => event.stopPropagation()}
          onClick={() => void desktopWindow.hide()}
        />
      </div>
    </header>
  );
}
