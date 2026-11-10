import { useEffect, useState } from "react";
import { Button, Caption1, Tooltip } from "@fluentui/react-components";
import { Dismiss16Regular, Maximize16Regular, Subtract16Regular, SquareMultiple16Regular } from "@fluentui/react-icons";
import { APP_NAME } from "./Poster";
import { inTauri } from "./api";

/**
 * The window has no system title bar (decorations are off), so this strip replaces it:
 * it drags the window, double-click maximises, and holds the three window buttons.
 * Outside the desktop app (browser preview) the buttons simply do nothing.
 */
export function TitleBar() {
  const [maximized, setMaximized] = useState(false);

  useEffect(() => {
    if (!inTauri) return;
    let unlisten: (() => void) | undefined;
    (async () => {
      const { getCurrentWindow } = await import("@tauri-apps/api/window");
      const win = getCurrentWindow();
      setMaximized(await win.isMaximized());
      unlisten = await win.onResized(async () => setMaximized(await win.isMaximized()));
    })();
    return () => unlisten?.();
  }, []);

  async function act(what: "minimize" | "toggleMaximize" | "close") {
    if (!inTauri) return;
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    await getCurrentWindow()[what]();
  }

  return (
    <header className="titlebar" data-tauri-drag-region>
      <div className="titlebar-brand" data-tauri-drag-region>
        <img src="/logo.svg" alt="" width={16} height={16} data-tauri-drag-region />
        <Caption1 data-tauri-drag-region>{APP_NAME}</Caption1>
      </div>
      <div className="titlebar-controls">
        <Tooltip content="Minimise" relationship="label">
          <Button className="wc" appearance="subtle" aria-label="Minimise" icon={<Subtract16Regular />} onClick={() => act("minimize")} />
        </Tooltip>
        <Tooltip content={maximized ? "Restore" : "Maximise"} relationship="label">
          <Button
            className="wc"
            appearance="subtle"
            aria-label={maximized ? "Restore" : "Maximise"}
            icon={maximized ? <SquareMultiple16Regular /> : <Maximize16Regular />}
            onClick={() => act("toggleMaximize")}
          />
        </Tooltip>
        <Tooltip content="Close" relationship="label">
          <Button className="wc wc-close" appearance="subtle" aria-label="Close" icon={<Dismiss16Regular />} onClick={() => act("close")} />
        </Tooltip>
      </div>
    </header>
  );
}
