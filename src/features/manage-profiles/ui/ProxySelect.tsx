import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { Button, Input, cn } from "@proxyshard/shardx-ui-kit";
import { AddIcon, ChevronDownIcon } from "../../../shared/icons";
import { toast } from "../../../shared/model/toast";
import { proxyBulkParse, proxySave, type ProxyEntry } from "../../../entities/proxy";
import { useProfile } from "../../../entities/profile";
import { storeBus } from "../../../shared/lib/storeBus";
import { useT } from "../../../shared/i18n";

const label = (p: ProxyEntry) =>
  p.name && p.name !== `${p.host}:${p.port}`
    ? `${p.name} · ${p.host}:${p.port}${p.country ? ` · ${p.country}` : ""}`
    : `${p.host}:${p.port} · ${p.country || p.kind}`;

type Coords = { left: number; width: number; top?: number; bottom?: number; maxHeight: number };

function useAnchoredCoords(open: boolean, trigger: React.RefObject<HTMLButtonElement | null>) {
  const [coords, setCoords] = useState<Coords | null>(null);
  useLayoutEffect(() => {
    if (!open) { setCoords(null); return; }
    const place = () => {
      const el = trigger.current;
      if (!el) return;
      const r = el.getBoundingClientRect();
      const gap = 4;
      const below = window.innerHeight - r.bottom;
      const above = r.top;
      // Drop upward when the row sits near the bottom of the window — the
      // editor is often the last row on the page.
      setCoords(
        below >= 240 || below >= above
          ? { left: r.left, width: r.width, top: r.bottom + gap, maxHeight: Math.min(320, below - gap - 8) }
          : { left: r.left, width: r.width, bottom: window.innerHeight - r.top + gap, maxHeight: Math.min(320, above - gap - 8) },
      );
    };
    place();
    window.addEventListener("resize", place);
    window.addEventListener("scroll", place, true);
    return () => {
      window.removeEventListener("resize", place);
      window.removeEventListener("scroll", place, true);
    };
  }, [open, trigger]);
  return coords;
}

/** The profile's proxy, with "create new" folded into the open list. */
export function ProxySelect({
  value, proxies, onChange,
}: {
  value: string | null;
  proxies: ProxyEntry[];
  onChange: (id: string | null) => void;
}) {
  const t = useT();
  const [open, setOpen] = useState(false);
  const [creating, setCreating] = useState(false);
  const [q, setQ] = useState("");
  const trigger = useRef<HTMLButtonElement>(null);
  const coords = useAnchoredCoords(open, trigger);
  const directLabel = t("proxySelect.directConnection");

  const close = () => { setOpen(false); setCreating(false); setQ(""); };
  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => { if (e.key === "Escape") close(); };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open]);

  const selected = value ? proxies.find((p) => p.id === value) ?? null : null;
  const needle = q.trim().toLowerCase();
  const shown = needle
    ? proxies.filter(
        (p) =>
          p.name.toLowerCase().includes(needle) ||
          p.host.toLowerCase().includes(needle) ||
          String(p.port).includes(needle) ||
          p.country.toLowerCase().includes(needle),
      )
    : proxies;

  const body = (
    <div
      style={{
        position: "fixed",
        left: coords?.left,
        width: coords?.width,
        top: coords?.top,
        bottom: coords?.bottom,
        zIndex: 1000,
      }}
      className="flex flex-col overflow-hidden rounded-xl bg-bg-white-0 shadow-[var(--shadow-md)] ring-1 ring-stroke-soft-200"
    >
      <button
        type="button"
        onClick={() => setCreating((v) => !v)}
        className={cn(
          "flex items-center gap-2 border-b border-stroke-soft-200 px-2.5 py-2 text-left text-paragraph-sm transition-colors",
          creating
            ? "bg-primary-alpha-10 text-primary-base"
            : "text-primary-base hover:bg-bg-weak-50",
        )}
      >
        <AddIcon className="size-4 shrink-0" />
        {t("proxySelect.createNew")}
      </button>

      {creating ? (
        <CreatePanel
          onCancel={() => setCreating(false)}
          onCreated={(p) => { onChange(p.id); close(); }}
        />
      ) : (
        <>
          {proxies.length > 8 && (
            <div className="border-b border-stroke-soft-200 px-2 py-1.5">
              <Input
                autoFocus
                inputSize="small"
                value={q}
                onChange={(e) => setQ(e.target.value)}
                placeholder={t("proxySelect.searchPlaceholder")}
              />
            </div>
          )}
          <ul className="overflow-auto p-1.5 scrollbar" style={{ maxHeight: coords?.maxHeight }}>
            <li>
              <Row
                text={directLabel}
                muted
                active={!value}
                onClick={() => { onChange(null); close(); }}
              />
            </li>
            {shown.map((p) => (
              <li key={p.id}>
                <Row
                  text={label(p)}
                  active={p.id === value}
                  onClick={() => { onChange(p.id); close(); }}
                />
              </li>
            ))}
            {shown.length === 0 && (
              <li className="px-2.5 py-4 text-center text-paragraph-sm text-text-soft-400">
                {t("proxySelect.noMatches")}
              </li>
            )}
          </ul>
        </>
      )}
    </div>
  );

  return (
    <>
      <button
        ref={trigger}
        type="button"
        onClick={() => (open ? close() : setOpen(true))}
        className="flex h-9 w-full items-center gap-2 rounded-lg bg-bg-white-0 px-2.5 text-left text-paragraph-sm text-text-strong-950 ring-1 ring-inset ring-stroke-soft-200 transition-colors hover:bg-bg-weak-50"
      >
        <span className={cn("min-w-0 flex-1 truncate", !selected && "text-text-soft-400")}>
          {selected ? label(selected) : directLabel}
        </span>
        <ChevronDownIcon
          className={cn("size-4 shrink-0 text-icon-soft-400 transition-transform", open && "rotate-180")}
        />
      </button>
      {open && coords && createPortal(
        <>
          <div className="fixed inset-0 z-[999]" onClick={close} />
          {body}
        </>,
        document.body,
      )}
    </>
  );
}

function Row({ text, active, muted, onClick }: {
  text: string;
  active: boolean;
  muted?: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={cn(
        "flex w-full items-center rounded-lg px-2.5 py-1.5 text-left text-paragraph-sm transition-colors",
        active
          ? "bg-primary-alpha-10 text-primary-base"
          : cn(muted ? "text-text-soft-400" : "text-text-sub-600", "hover:bg-bg-weak-50"),
      )}
    >
      <span className="min-w-0 flex-1 truncate">{text}</span>
    </button>
  );
}

/** Paste one line; parsed by the same code bulk import uses. */
function CreatePanel({ onCancel, onCreated }: {
  onCancel: () => void;
  onCreated: (p: ProxyEntry) => void;
}) {
  const t = useT();
  const [line, setLine] = useState("");
  const [parsed, setParsed] = useState<ProxyEntry | null>(null);
  const [busy, setBusy] = useState(false);
  const reloadProfiles = useProfile((s) => s.reload);

  // Parsed as you type, so the preview says what was understood before
  // anything is saved. Debounced — each attempt is a round trip into Rust.
  useEffect(() => {
    const text = line.trim();
    if (!text) { setParsed(null); return; }
    let alive = true;
    const t = setTimeout(() => {
      proxyBulkParse(text, "socks5")
        .then((rows) => { if (alive) setParsed(rows[0] ?? null); })
        .catch(() => { if (alive) setParsed(null); });
    }, 180);
    return () => { alive = false; clearTimeout(t); };
  }, [line]);

  const save = async () => {
    if (!parsed) return;
    setBusy(true);
    try {
      const saved = await proxySave(parsed);
      // The Proxies page keeps its own copy of the list; tell it too.
      storeBus.emit("proxies");
      // Awaited, because the select reads the profile store's copy and has to
      // hold the new proxy before it is selected.
      await reloadProfiles();
      onCreated(saved);
    } catch (e) { toast.err(String(e)); }
    finally { setBusy(false); }
  };

  return (
    <div className="flex flex-col gap-2 p-2">
      <Input
        autoFocus
        inputSize="small"
        className="mono"
        value={line}
        onChange={(e) => setLine(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter" && parsed && !busy) { e.preventDefault(); void save(); }
        }}
        placeholder={t("proxySelect.linePlaceholder")}
      />
      <div className="min-h-[34px] rounded-8 bg-bg-weak-50 px-2.5 py-1.5 text-paragraph-xs ring-1 ring-inset ring-stroke-soft-200">
        {parsed ? (
          <span className="text-text-sub-600">
            <strong className="text-text-strong-950">{parsed.name}</strong>
            {" · "}
            {parsed.kind.toUpperCase()} {parsed.host}:{parsed.port}
            {parsed.username && ` · ${parsed.username}`}
          </span>
        ) : (
          <span className="text-text-soft-400">
            {line.trim()
              ? t("proxySelect.parseFailed")
              : t("proxySelect.pasteHint")}
          </span>
        )}
      </div>
      <div className="flex justify-end gap-2">
        <Button variant="neutral" mode="ghost" size="2xsmall" onClick={onCancel}>
          {t("proxySelect.cancel")}
        </Button>
        <Button
          variant="primary" mode="filled" size="2xsmall"
          disabled={!parsed || busy} isLoading={busy}
          onClick={save}
        >
          {t("proxySelect.addAndBind")}
        </Button>
      </div>
    </div>
  );
}
