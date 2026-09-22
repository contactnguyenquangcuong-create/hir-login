import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { Button } from "@proxyshard/shardx-ui-kit";
import { CloseIcon, PlayIcon } from "../../shared/icons";
import { toast } from "../../shared/lib/toast";
import {
  automationCall,
  automationDetach,
  automationLaunch,
  automationPick,
  automationScreencast,
  type Frame,
  type Picked,
} from "../../entities/automation";
import { processKill } from "../../entities/profile";
import { ActionMenu, type MenuAction } from "./ActionMenu";
import { useT } from "../../shared/i18n";

const MAX_W = 1280;
const MAX_H = 800;

type Props = {
  profileId: string | null;
  /** Adds a step. `target` is null when nothing resolvable was under the click. */
  onAction?: (action: MenuAction, target: Picked | null) => void;
  /** Records an action the operator just performed on the page. */
  onRecorded?: (kind: string, target: Picked | null, extra?: Record<string, unknown>) => void;
  recording: boolean;
  onRecording: (on: boolean) => void;
};

/** Keys the page should get as a named key rather than as text. Everything
 *  else that produces a character is sent as that character, so the profile's
 *  own keyboard layout decides which physical key it came from. */
const NAMED = new Set([
  "Enter", "Tab", "Escape", "Backspace", "Delete", "Home", "End",
  "PageUp", "PageDown", "ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight",
  "F1", "F2", "F3", "F4", "F5", "F6", "F7", "F8", "F9", "F10", "F11", "F12",
]);

/** The profile's own window, mirrored frame by frame. Not an embedded browser:
 *  the page runs in the real engine and only its pixels come here. */
export function LiveView({
  profileId,
  onAction,
  onRecorded,
  recording,
  onRecording,
}: Props) {
  const t = useT();
  const [menu, setMenu] = useState<{ at: { x: number; y: number }; target: Picked | null } | null>(null);
  const canvas = useRef<HTMLCanvasElement | null>(null);
  const meta = useRef<Frame | null>(null);
  const [live, setLive] = useState(false);
  const [busy, setBusy] = useState(false);
  // The frame's own pixel ratio, so the view can size a box to it and grow the
  // browser to fill the column instead of sitting tiny inside letterbox bars.
  const [dim, setDim] = useState({ w: 16, h: 10 });

  useEffect(() => {
    let stop: (() => void) | null = null;
    let alive = true;
    listen<Frame>("automation:frame", (e) => {
      if (!alive || !profileId || e.payload.profile_id !== profileId) return;
      meta.current = e.payload;
      const el = canvas.current;
      if (!el) return;
      const img = new Image();
      img.onload = () => {
        if (el.width !== img.width || el.height !== img.height) {
          el.width = img.width;
          el.height = img.height;
          setDim({ w: img.width, h: img.height });
        }
        el.getContext("2d")?.drawImage(img, 0, 0);
      };
      img.src = `data:image/jpeg;base64,${e.payload.data}`;
    }).then((un) => { stop = un; });
    return () => { alive = false; stop?.(); };
  }, [profileId]);

  // A navigation the operator caused becomes an "Open URL" step. Only while
  // recording, and only the top frame — the browser tells us which.
  useEffect(() => {
    let stop: (() => void) | null = null;
    let alive = true;
    listen<{ profile_id: string; url: string }>("automation:navigated", (e) => {
      if (!alive || !recording || !profileId) return;
      if (e.payload.profile_id !== profileId) return;
      const url = e.payload.url;
      if (!url || url === "about:blank" || url.startsWith("chrome")) return;
      onRecorded?.("goto", null, { url });
    }).then((un) => { stop = un; });
    return () => { alive = false; stop?.(); };
  }, [profileId, recording, onRecorded]);

  const start = useCallback(async () => {
    if (!profileId) return;
    setBusy(true);
    try {
      await automationLaunch(profileId);
      await automationScreencast(profileId, true, MAX_W, MAX_H);
      setLive(true);
    } catch (e) { toast.err(String(e)); }
    finally { setBusy(false); }
  }, [profileId]);

  const stop = useCallback(async () => {
    if (!profileId) return;
    try { await automationScreencast(profileId, false, MAX_W, MAX_H); } catch { /* already gone */ }
    automationDetach(profileId);
    // Actually close the browser — Stop used to only end the screencast and
    // detach CDP, leaving the window open.
    try { await processKill(profileId); } catch { /* already closed */ }
    setLive(false);
  }, [profileId]);

  // Canvas pixels -> page CSS pixels. The frame is scaled to fit maxWidth, so
  // the ratio is the frame's own width against the page width it covers.
  // Where on the PAGE a click on the canvas landed.
  //
  // The canvas is drawn with object-contain, so the frame sits centred inside
  // the element with bars on two sides — the element's own rectangle is not
  // where the picture is. Measuring against the rectangle put every click a
  // little (and near the edges, a lot) off target, which is why recorded steps
  // came out as coordinates: the element resolver was asked about the wrong
  // point and found nothing.
  const toPage = (e: React.MouseEvent<HTMLCanvasElement>) => {
    const el = canvas.current;
    const m = meta.current;
    if (!el || !m || !el.width || !el.height) return null;
    const r = el.getBoundingClientRect();
    if (!r.width || !r.height) return null;

    const shown = Math.min(r.width / el.width, r.height / el.height);
    const padX = (r.width - el.width * shown) / 2;
    const padY = (r.height - el.height * shown) / 2;

    const imgX = (e.clientX - r.left - padX) / shown;
    const imgY = (e.clientY - r.top - padY) / shown;
    // A click on the bars is not a click on the page.
    if (imgX < 0 || imgY < 0 || imgX > el.width || imgY > el.height) return null;

    // The frame is a scaled shot of the visible page, so one factor per axis.
    const kx = m.width > 0 ? m.width / el.width : 1;
    const ky = m.height > 0 ? m.height / el.height : 1;
    return { x: imgX * kx, y: imgY * ky };
  };

  // Left click drives the page through Motion, so what the site sees is a
  // human pointer rather than a synthetic event.
  const onClick = async (e: React.MouseEvent<HTMLCanvasElement>) => {
    if (!profileId || !live) return;
    const p = toPage(e);
    if (!p) return;
    // While recording, resolve the element BEFORE clicking: the click may
    // navigate, and then there is nothing left under the point to identify.
    let target: Picked | null = null;
    if (recording) {
      // Not swallowed: a pick that fails is why a step ends up as coordinates,
      // and silently falling back hid that for far too long.
      target = await automationPick(profileId, p.x, p.y).catch((err) => {
        toast.err(`Could not identify that element: ${err}`);
        return null;
      });
    }
    try {
      await automationCall(profileId, "Motion.createPointer", { x: p.x, y: p.y });
      await automationCall(profileId, "Motion.glideTo", { x: p.x, y: p.y });
      await automationCall(profileId, "Motion.tap", {});
      if (recording) onRecorded?.("click", target ?? { selector: "", tag: "", label: "", x: p.x, y: p.y, tried: [] });
    } catch (err) { toast.err(String(err)); }
  };

  // The canvas is focusable, so it receives keys while the operator is looking
  // at it. Each key goes out through Motion, which resolves it against the
  // profile's own layout — a ru-RU profile pressing "a" reports the physical
  // key its keyboard would have used.
  const onKeyDown = async (e: React.KeyboardEvent<HTMLCanvasElement>) => {
    if (!profileId || !live) return;
    if (e.key === "Unidentified") return;
    const mods: string[] = [];
    if (e.shiftKey) mods.push("Shift");
    if (e.ctrlKey) mods.push("Control");
    if (e.altKey) mods.push("Alt");
    if (e.metaKey) mods.push("Meta");

    let key: string | null = null;
    if (NAMED.has(e.key)) key = e.key;
    else if (e.key.length === 1) key = e.key;
    if (!key) return;

    // Shift is already carried by the character itself; naming it too would
    // press Shift twice for a capital letter.
    const send = mods.filter((m) => !(m === "Shift" && key!.length === 1));

    e.preventDefault();
    try {
      await automationCall(profileId, "Motion.pressKey", {
        key,
        ...(send.length ? { modifiers: send } : {}),
      });
      if (recording) {
        onRecorded?.("press", null, {
          key,
          ...(send.length ? { modifiers: send.join(", ") } : {}),
        });
      }
    } catch (err) { toast.err(String(err)); }
  };

  // Right-click belongs to the automator only while recording; otherwise it
  // goes to the page, where it opens the site's own menu.
  // Right-click is the automator's, always. Shift is the way through to the
  // page's own menu — the reverse would mean the operator had to switch a mode
  // on just to add a step, which is the thing they do most.
  const onContext = async (e: React.MouseEvent<HTMLCanvasElement>) => {
    e.preventDefault();
    if (!profileId || !live) return;
    const p = toPage(e);
    if (!p) return;

    if (e.shiftKey) {
      try {
        await automationCall(profileId, "Motion.createPointer", { x: p.x, y: p.y });
        await automationCall(profileId, "Motion.glideTo", { x: p.x, y: p.y });
        await automationCall(profileId, "Motion.tap", { button: "right" });
      } catch (err) { toast.err(String(err)); }
      return;
    }

    const at = { x: e.clientX, y: e.clientY };
    setMenu({ at, target: null });
    const target = await automationPick(profileId, p.x, p.y).catch((err) => {
      toast.err(`Could not identify that element: ${err}`);
      return null;
    });
    setMenu((m) => (m ? { ...m, target: target ?? { selector: "", tag: "", label: "", x: p.x, y: p.y, tried: [] } } : m));
  };

  if (!profileId) {
    return (
      <div className="flex h-full items-center justify-center rounded-12 bg-bg-weak-50 px-6 text-center ring-1 ring-inset ring-stroke-soft-200">
        <p className="m-0 text-paragraph-sm text-text-soft-400">
          {t("liveView.pickProfile")}
        </p>
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col gap-2">
      <div className="flex items-center gap-2">
        {live ? (
          <Button
            variant="neutral" mode="stroke" size="xsmall"
            leftIcon={<CloseIcon className="size-4" />}
            onClick={stop}
          >
            {t("liveView.stop")}
          </Button>
        ) : (
          <Button
            variant="primary" mode="filled" size="xsmall"
            leftIcon={<PlayIcon className="size-4" />}
            disabled={busy}
            onClick={start}
          >
            {busy ? "Starting…" : "Open browser"}
          </Button>
        )}
        {live && (
          <>
            <Button
              variant={recording ? "error" : "neutral"}
              mode={recording ? "filled" : "stroke"}
              size="xsmall"
              onClick={() => onRecording(!recording)}
            >
              {recording ? "Stop recording" : "Record steps"}
            </Button>
            <span className="text-paragraph-xs text-text-soft-400">
              {recording
                ? "Everything you do is being written down"
                : "Right-click to add a step · Shift+right-click opens the page's own menu"}
            </span>
          </>
        )}
      </div>
      <div className="flex flex-1 items-center justify-center overflow-hidden rounded-12 bg-bg-weak-50 ring-1 ring-inset ring-stroke-soft-200">
        {/* A box the size of the frame's aspect ratio, grown to the largest that
            fits — so the browser fills the column, not a tiny letterboxed strip. */}
        <div className="max-h-full max-w-full" style={{ width: "100%", aspectRatio: `${dim.w} / ${dim.h}` }}>
          <canvas
            ref={canvas}
            tabIndex={0}
            className="h-full w-full object-contain outline-none"
            onClick={onClick}
            onContextMenu={onContext}
            onKeyDown={onKeyDown}
          />
        </div>
      </div>

      {menu && (
        <ActionMenu
          at={menu.at}
          target={menu.target}
          onChoose={(a) => onAction?.(a, menu.target)}
          onClose={() => setMenu(null)}
        />
      )}
    </div>
  );
}
