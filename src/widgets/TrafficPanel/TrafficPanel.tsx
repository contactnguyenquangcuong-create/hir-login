import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { Button } from "@proxyshard/shardx-ui-kit";
import { toast } from "../../shared/lib/toast";
import { useT } from "../../shared/i18n";
import {
  trafficMount,
  trafficClear,
  trafficObserve,
  trafficResolve,
  type TrafficAction,
  type TrafficHeaderOp,
  type TrafficRule,
  type TrafficPaused,
} from "../../entities/automation";

type Observed = {
  id: string;
  url: string;
  method: string;
  resource: string;
  capture?: boolean;
  status?: number;
  requestHeaders?: Record<string, string>;
  requestBody?: string;
  responseHeaders?: Record<string, string>;
  responseBody?: string;
  responseBodyTruncated?: boolean;
};

const headersToOps = (h?: Record<string, string>) =>
  Object.entries(h ?? {}).map(([name, value]) => ({ name, value }));

// A header map as the "Name: value" lines the traffic step blocks take.
const recordToText = (h?: Record<string, string>) =>
  Object.entries(h ?? {}).map(([k, v]) => `${k}: ${v}`).join("\n");

const hostOf = (url: string) => {
  try { return new URL(url).host; } catch { return url; }
};

const FIELD =
  "h-8 w-full rounded-8 bg-bg-white-0 px-2 text-paragraph-sm text-text-strong-950 ring-1 ring-inset ring-stroke-soft-200 outline-none focus:ring-primary-base";
const AREA =
  "min-h-16 w-full rounded-8 bg-bg-white-0 p-2 font-mono text-[11px] text-text-strong-950 ring-1 ring-inset ring-stroke-soft-200 outline-none focus:ring-primary-base";
const LABEL = "text-subheading-2xs text-text-soft-400";

const METHODS = ["", "GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"];
const KINDS = ["", "document", "xhr", "script", "stylesheet", "image", "font", "media", "websocket", "other"];
const TYPES: TrafficAction["type"][] = ["continue", "modify", "block", "redirect", "fulfill"];

// headers <-> "Name: Value" lines
const headersToText = (h?: TrafficHeaderOp[]) =>
  (h ?? []).map((o) => `${o.name}: ${o.value}`).join("\n");
const textToHeaders = (t: string): TrafficHeaderOp[] =>
  t.split("\n").map((l) => l.trim()).filter(Boolean).map((l) => {
    const i = l.indexOf(":");
    return i < 0
      ? { name: l, value: "" }
      : { name: l.slice(0, i).trim(), value: l.slice(i + 1).trim() };
  });
const csvToList = (t: string) => t.split(",").map((s) => s.trim()).filter(Boolean);

function emptyRule(): TrafficRule {
  return { match: { url: "" }, action: { type: "block" } };
}

function RuleCard({
  rule,
  onChange,
  onRemove,
}: {
  rule: TrafficRule;
  onChange: (r: TrafficRule) => void;
  onRemove: () => void;
}) {
  const t = useT();
  const m = rule.match;
  const a = rule.action;
  const regex = m.urlRegex !== undefined || m.hostRegex !== undefined;
  const setMatch = (patch: Partial<typeof m>) => onChange({ ...rule, match: { ...m, ...patch } });
  const setAction = (patch: Partial<TrafficAction>) => onChange({ ...rule, action: { ...a, ...patch } });

  const toggleRegex = (on: boolean) => {
    if (on) {
      onChange({ ...rule, match: { method: m.method, resource: m.resource, urlRegex: m.url ?? "", hostRegex: m.host ?? "" } });
    } else {
      onChange({ ...rule, match: { method: m.method, resource: m.resource, url: m.urlRegex ?? "", host: m.hostRegex ?? "" } });
    }
  };

  return (
    <div className="flex flex-col gap-2 rounded-8 bg-bg-weak-50 p-2.5">
      {/* match */}
      <div className="flex items-center gap-2">
        <span className="text-label-sm text-text-strong-950">{t("trafficPanel.whenLabel")}</span>
        <select className={FIELD + " w-auto"} value={m.method ?? ""} onChange={(e) => setMatch({ method: e.target.value || undefined })}>
          {METHODS.map((x) => <option key={x} value={x}>{x || t("trafficPanel.anyMethod")}</option>)}
        </select>
        <select className={FIELD + " w-auto"} value={m.resource ?? ""} onChange={(e) => setMatch({ resource: e.target.value || undefined })}>
          {KINDS.map((x) => <option key={x} value={x}>{x || t("trafficPanel.anyType")}</option>)}
        </select>
        <label className="ml-auto flex items-center gap-1 text-paragraph-xs text-text-soft-400">
          <input type="checkbox" checked={regex} onChange={(e) => toggleRegex(e.target.checked)} /> {t("trafficPanel.regexToggle")}
        </label>
      </div>
      <div className="grid grid-cols-1 gap-2">
        <label className="flex flex-col gap-1">
          <span className={LABEL}>{regex ? t("trafficPanel.urlRegexLabel") : t("trafficPanel.urlGlobLabel")}</span>
          <input className={FIELD} value={(regex ? m.urlRegex : m.url) ?? ""} placeholder={regex ? "^https://.*/api/" : "*://*.example.com/*"}
            onChange={(e) => setMatch(regex ? { urlRegex: e.target.value } : { url: e.target.value })} />
        </label>
        <label className="flex flex-col gap-1">
          <span className={LABEL}>{regex ? t("trafficPanel.hostRegexLabel") : t("trafficPanel.hostGlobLabel")}</span>
          <input className={FIELD} value={(regex ? m.hostRegex : m.host) ?? ""} placeholder={regex ? "" : "*.example.com"}
            onChange={(e) => setMatch(regex ? { hostRegex: e.target.value } : { host: e.target.value })} />
        </label>
      </div>

      {/* action */}
      <div className="flex items-center gap-2">
        <span className="text-label-sm text-text-strong-950">{t("trafficPanel.doLabel")}</span>
        <select className={FIELD + " w-auto"} value={a.type} onChange={(e) => setAction({ type: e.target.value as TrafficAction["type"] })}>
          {TYPES.map((x) => <option key={x} value={x}>{x}</option>)}
        </select>
        <label className="ml-auto flex items-center gap-1 text-paragraph-xs text-text-soft-400">
          <input type="checkbox" checked={!!rule.await} onChange={(e) => onChange({ ...rule, await: e.target.checked })} /> {t("trafficPanel.awaitToggle")}
        </label>
      </div>

      {a.type === "block" && (
        <label className="flex flex-col gap-1">
          <span className={LABEL}>{t("trafficPanel.blockReasonLabel")}</span>
          <input className={FIELD} value={a.blockReason ?? ""} placeholder="BlockedByClient / AccessDenied / …"
            onChange={(e) => setAction({ blockReason: e.target.value || undefined })} />
        </label>
      )}

      {a.type === "redirect" && (
        <label className="flex flex-col gap-1">
          <span className={LABEL}>{t("trafficPanel.redirectToLabel")}</span>
          <input className={FIELD} value={a.setUrl ?? ""} placeholder="https://…"
            onChange={(e) => setAction({ setUrl: e.target.value })} />
        </label>
      )}

      {a.type === "fulfill" && (
        <div className="flex flex-col gap-2">
          <div className="grid grid-cols-1 gap-2">
            <label className="flex flex-col gap-1">
              <span className={LABEL}>{t("trafficPanel.statusLabel")}</span>
              <input className={FIELD} type="number" value={a.status ?? 200} onChange={(e) => setAction({ status: Number(e.target.value) })} />
            </label>
            <label className="flex flex-col gap-1">
              <span className={LABEL}>{t("trafficPanel.responseHeadersLabel")}</span>
              <textarea className={AREA} value={headersToText(a.responseHeaders)} placeholder="Content-Type: application/json"
                onChange={(e) => setAction({ responseHeaders: textToHeaders(e.target.value) })} />
            </label>
          </div>
          <label className="flex flex-col gap-1">
            <span className={LABEL}>{t("trafficPanel.responseBodyLabel")}</span>
            <textarea className={AREA} value={a.responseBody ?? ""} onChange={(e) => setAction({ responseBody: e.target.value })} />
          </label>
        </div>
      )}

      {(a.type === "modify" || a.type === "continue") && (
        <div className="flex flex-col gap-2">
          <div className="text-subheading-2xs text-text-soft-400">{t("trafficPanel.requestSection")}</div>
          <div className="grid grid-cols-1 gap-2">
            <label className="flex flex-col gap-1">
              <span className={LABEL}>{t("trafficPanel.setMethodLabel")}</span>
              <input className={FIELD} value={a.setMethod ?? ""} onChange={(e) => setAction({ setMethod: e.target.value || undefined })} />
            </label>
            <label className="flex flex-col gap-1">
              <span className={LABEL}>{t("trafficPanel.rewriteUrlLabel")}</span>
              <input className={FIELD} value={a.setUrl ?? ""} onChange={(e) => setAction({ setUrl: e.target.value || undefined })} />
            </label>
          </div>
          <label className="flex flex-col gap-1">
            <span className={LABEL}>{t("trafficPanel.setRequestHeadersLabel")}</span>
            <textarea className={AREA} value={headersToText(a.setHeaders)} onChange={(e) => setAction({ setHeaders: textToHeaders(e.target.value) })} />
          </label>
          <div className="grid grid-cols-1 gap-2">
            <label className="flex flex-col gap-1">
              <span className={LABEL}>{t("trafficPanel.removeRequestHeadersLabel")}</span>
              <input className={FIELD} value={(a.removeHeaders ?? []).join(", ")} onChange={(e) => setAction({ removeHeaders: csvToList(e.target.value) })} />
            </label>
            <label className="flex flex-col gap-1">
              <span className={LABEL}>{t("trafficPanel.delayMsLabel")}</span>
              <input className={FIELD} type="number" value={a.delayMs ?? 0} onChange={(e) => setAction({ delayMs: Number(e.target.value) || undefined })} />
            </label>
          </div>
          <div className="text-subheading-2xs text-text-soft-400">{t("trafficPanel.responseSection")}</div>
          <label className="flex flex-col gap-1">
            <span className={LABEL}>{t("trafficPanel.setResponseHeadersLabel")}</span>
            <textarea className={AREA} value={headersToText(a.setResponseHeaders)} onChange={(e) => setAction({ setResponseHeaders: textToHeaders(e.target.value) })} />
          </label>
          <div className="grid grid-cols-1 gap-2">
            <label className="flex flex-col gap-1">
              <span className={LABEL}>{t("trafficPanel.removeResponseHeadersLabel")}</span>
              <input className={FIELD} value={(a.removeResponseHeaders ?? []).join(", ")} onChange={(e) => setAction({ removeResponseHeaders: csvToList(e.target.value) })} />
            </label>
            <label className="flex flex-col gap-1">
              <span className={LABEL}>{t("trafficPanel.setStatusLabel")}</span>
              <input className={FIELD} type="number" value={a.setStatus ?? ""} onChange={(e) => setAction({ setStatus: e.target.value ? Number(e.target.value) : undefined })} />
            </label>
          </div>
          <label className="flex flex-col gap-1">
            <span className={LABEL}>{t("trafficPanel.replaceResponseBodyLabel")}</span>
            <textarea className={AREA} value={a.setResponseBody ?? ""} onChange={(e) => setAction({ setResponseBody: e.target.value || undefined })} />
          </label>
        </div>
      )}

      <button className="self-start text-paragraph-xs text-error-base hover:underline" onClick={onRemove}>{t("trafficPanel.removeRule")}</button>
    </div>
  );
}

export function TrafficPanel({
  rules,
  onRulesChange,
  onAddStep,
  profileId,
  live,
}: {
  rules: TrafficRule[];
  onRulesChange: (rules: TrafficRule[]) => void;
  /** Drops a traffic step block into the project's flow (the "кубик"). */
  onAddStep?: (kind: string, params: Record<string, unknown>, label: string) => void;
  profileId: string | null;
  live: boolean;
}) {
  const t = useT();
  const [mounted, setMounted] = useState<string | null>(null);
  const [paused, setPaused] = useState<TrafficPaused[]>([]);
  const [observing, setObserving] = useState(false);
  const [observed, setObserved] = useState<Observed[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [menu, setMenu] = useState<{ x: number; y: number; item: Observed } | null>(null);

  useEffect(() => {
    let un: (() => void) | null = null;
    let alive = true;
    listen<{ profile_id: string; params: { requestId: string; request: string } }>(
      "automation:traffic-paused",
      (e) => {
        if (!alive || !profileId || e.payload.profile_id !== profileId) return;
        let request: TrafficPaused["request"];
        try { request = JSON.parse(e.payload.params.request); }
        catch { request = { url: "?", method: "?", resource: "?", headers: {} }; }
        setPaused((prev) => [{ requestId: e.payload.params.requestId, request }, ...prev].slice(0, 50));
      },
    ).then((u) => { un = u; });
    return () => { alive = false; un?.(); };
  }, [profileId]);

  useEffect(() => {
    let un: (() => void) | null = null;
    let alive = true;
    listen<{ profile_id: string; params: { request: string } }>(
      "automation:traffic-observed",
      (e) => {
        if (!alive || !profileId || e.payload.profile_id !== profileId) return;
        let ev: any;
        try { ev = JSON.parse(e.payload.params.request); } catch { return; }
        const id: string = ev.id ?? "";
        setObserved((prev) => {
          const i = prev.findIndex((r) => r.id === id);
          if (ev.phase === "response") {
            if (i < 0) return prev;
            const next = prev.slice();
            next[i] = { ...prev[i], status: ev.status, requestHeaders: ev.requestHeaders,
              requestBody: ev.requestBody, responseHeaders: ev.responseHeaders,
              responseBody: ev.responseBody, responseBodyTruncated: ev.responseBodyTruncated };
            return next;
          }
          if (i >= 0) return prev;
          return [{ id, url: ev.url, method: ev.method, resource: ev.resource, capture: ev.capture }, ...prev].slice(0, 300);
        });
      },
    ).then((u) => { un = u; });
    return () => { alive = false; un?.(); };
  }, [profileId]);

  // Close the context menu on any outside click.
  useEffect(() => {
    if (!menu) return;
    const close = () => setMenu(null);
    window.addEventListener("click", close);
    return () => window.removeEventListener("click", close);
  }, [menu]);

  const toggleObserve = async () => {
    if (!profileId) return;
    const next = !observing;
    try {
      await trafficObserve(profileId, next);
      setObserving(next);
      if (!next) setObserved([]);
    } catch (e) { toast.err(String(e)); }
  };

  // Adds a traffic step block (the "кубик") to the project flow, prefilled from
  // this request. Put it before the step that triggers the request and it
  // mounts there when the run reaches it. This is the persistent, manageable way
  // — the rule lives in the flow like any other step.
  const addStepFromRequest = (o: Observed, kind: string) => {
    const base: Record<string, unknown> = {
      url: o.url,
      ...(o.method ? { method: o.method } : {}),
      ...(o.resource && o.resource !== "any" ? { resource: o.resource } : {}),
    };
    let label = t("trafficPanel.stepLabelTraffic");
    const params: Record<string, unknown> = { ...base };
    if (kind === "traffic.block") {
      label = t("trafficPanel.stepLabelBlock");
    } else if (kind === "traffic.redirect") {
      params.to = "";
      label = t("trafficPanel.stepLabelRedirect");
    } else if (kind === "traffic.setHeaders") {
      if (o.requestHeaders) params.headers = recordToText(o.requestHeaders);
      if (o.requestBody) params.setBody = o.requestBody;
      label = t("trafficPanel.stepLabelRewriteRequest");
    } else if (kind === "traffic.editResponse") {
      if (o.status) params.status = o.status;
      if (o.responseHeaders) params.responseHeaders = recordToText(o.responseHeaders);
      if (o.responseBody != null) params.responseBody = o.responseBody;
      label = t("trafficPanel.stepLabelEditResponse");
    } else if (kind === "traffic.fulfill") {
      params.status = o.status ?? 200;
      if (o.responseHeaders) params.responseHeaders = recordToText(o.responseHeaders);
      params.responseBody = o.responseBody ?? "";
      label = t("trafficPanel.stepLabelFakeResponse");
    }
    if (!onAddStep) { toast.err(t("trafficPanel.cannotAddStep")); return; }
    onAddStep(kind, params, `${label} ${hostOf(o.url)}`);
    setMenu(null);
    toast.ok(t("trafficPanel.stepAdded"));
  };

  // Builds a live rule ({match, action}) from a request — for the test-only
  // "Mount live" action. Not saved anywhere; it just applies to the running
  // browser now so you can check the rule before wiring it into the flow.
  const ruleFromRequest = (o: Observed, type: TrafficAction["type"]): TrafficRule => {
    const match = {
      url: o.url,
      ...(o.method ? { method: o.method } : {}),
      ...(o.resource && o.resource !== "any" ? { resource: o.resource } : {}),
    };
    let action: TrafficAction = { type };
    if (type === "modify") {
      action = {
        type: "modify",
        ...(o.responseHeaders ? { setResponseHeaders: headersToOps(o.responseHeaders) } : {}),
        ...(o.status ? { setStatus: o.status } : {}),
      };
    } else if (type === "fulfill") {
      action = {
        type: "fulfill",
        status: o.status ?? 200,
        ...(o.responseHeaders ? { responseHeaders: headersToOps(o.responseHeaders) } : {}),
        responseBody: o.responseBody ?? "",
      };
    }
    return { match, action };
  };

  const mountLiveFromRequest = async (o: Observed, type: TrafficAction["type"]) => {
    setMenu(null);
    if (!profileId) { toast.err(t("trafficPanel.noLiveProfile")); return; }
    try {
      await trafficMount(profileId, [ruleFromRequest(o, type)]);
      setMounted("live");
      toast.ok(t("trafficPanel.mountedLiveTest"));
    } catch (e) { toast.err(String(e)); }
  };

  const update = (i: number, r: TrafficRule) =>
    onRulesChange(rules.map((x, j) => (j === i ? r : x)));
  const remove = (i: number) => onRulesChange(rules.filter((_, j) => j !== i));
  const add = () => onRulesChange([...rules, emptyRule()]);

  const selected = observed.find((r) => r.id === selectedId) ?? null;

  const mount = async () => {
    if (!profileId) return;
    try {
      if (mounted) await trafficClear(profileId).catch(() => {});
      const r = await trafficMount(profileId, rules);
      setMounted(r.ruleSetId);
      toast.ok(t("trafficPanel.mountedRules", { n: rules.length }));
    } catch (e) { toast.err(String(e)); }
  };
  const clear = async () => {
    if (!profileId) return;
    try { await trafficClear(profileId); setMounted(null); toast.ok(t("trafficPanel.clearedToast")); }
    catch (e) { toast.err(String(e)); }
  };

  const resolve = async (p: TrafficPaused, action?: TrafficAction) => {
    if (!profileId) return;
    try {
      await trafficResolve(profileId, p.requestId, action);
      setPaused((prev) => prev.filter((x) => x.requestId !== p.requestId));
    } catch (e) { toast.err(String(e)); }
  };

  return (
    <div className="flex flex-col gap-3">
      <div className="flex flex-wrap items-center gap-2">
        <div className="text-label-sm text-text-strong-950">{t("trafficPanel.title")}</div>
        <div className="ml-auto flex flex-wrap items-center gap-1.5">
          {mounted && <span className="text-paragraph-xs text-success-base">{t("trafficPanel.mountedBadge")}</span>}
          <Button size="xsmall" mode="stroke" onClick={add}>{t("trafficPanel.addRule")}</Button>
          <Button size="xsmall" mode={observing ? "filled" : "stroke"} disabled={!live} onClick={toggleObserve}>
            {observing ? t("trafficPanel.observing") : t("trafficPanel.observe")}
          </Button>
          <Button size="xsmall" disabled={!live} onClick={mount}>{t("trafficPanel.mountToLive")}</Button>
          {mounted && <Button size="xsmall" mode="stroke" disabled={!live} onClick={clear}>{t("trafficPanel.clearButton")}</Button>}
        </div>
      </div>
      <p className="m-0 text-paragraph-xs text-text-soft-400">
        {t("trafficPanel.savedHint")}
      </p>
      {!live && (
        <div className="rounded-8 bg-warning-alpha-16 px-2 py-1 text-[11px] text-warning-base">
          {t("trafficPanel.liveRequiredHint")}
        </div>
      )}

      {rules.length === 0 && (
        <p className="m-0 py-6 text-center text-paragraph-xs text-text-soft-400">{t("trafficPanel.noRules")}</p>
      )}
      {rules.map((r, i) => (
        <RuleCard key={i} rule={r} onChange={(nr) => update(i, nr)} onRemove={() => remove(i)} />
      ))}

      {/* observed traffic — right-click a row to seed a rule */}
      {(observing || observed.length > 0) && (
        <>
          <div className="mt-2 flex items-center gap-2">
            <div className="text-label-sm text-text-strong-950">{t("trafficPanel.requestsTitle")}</div>
            {observed.length > 0 && (
              <>
                <span className="text-paragraph-xs text-text-soft-400">{observed.length}</span>
                <button className="ml-auto text-paragraph-xs text-text-soft-400 hover:text-text-strong-950" onClick={() => { setObserved([]); setSelectedId(null); }}>{t("trafficPanel.clearRequests")}</button>
              </>
            )}
          </div>
          <p className="m-0 text-paragraph-xs text-text-soft-400">{t("trafficPanel.rightClickHint")}</p>
          <div className="flex max-h-[220px] flex-col gap-0.5 overflow-y-auto">
            {observed.map((o) => (
              <div
                key={o.id}
                onClick={() => setSelectedId(o.id)}
                onContextMenu={(e) => { e.preventDefault(); setSelectedId(o.id); setMenu({ x: e.clientX, y: e.clientY, item: o }); }}
                className={"flex cursor-pointer items-center gap-2 rounded-6 px-1.5 py-1 hover:bg-bg-weak-50 " + (selectedId === o.id ? "bg-bg-weak-50 ring-1 ring-inset ring-stroke-soft-200" : "")}
                title={o.url}
              >
                <span className="w-10 shrink-0 font-mono text-[10px] text-text-soft-400">{o.method}</span>
                <span className="truncate text-[11px] text-text-strong-950">{o.url}</span>
                {o.status ? <span className="shrink-0 text-[10px] text-text-soft-400">{o.status}</span> : null}
                <span className="ml-auto shrink-0 text-[10px] text-text-soft-400">{o.resource}</span>
              </div>
            ))}
          </div>
          {selected && (
            <div className="mt-1 flex flex-col gap-2 rounded-8 bg-bg-weak-50 p-2 text-[11px]">
              <div className="flex items-center gap-2">
                <span className="font-mono text-text-soft-400">{selected.method}</span>
                {selected.status ? <span className="rounded-6 bg-bg-white-0 px-1 text-text-strong-950">{selected.status}</span> : <span className="text-text-soft-400">…</span>}
                <span className="text-[10px] text-text-soft-400">{selected.resource}</span>
                <button className="ml-auto text-text-soft-400 hover:text-text-strong-950" onClick={() => setSelectedId(null)}>{t("trafficPanel.closeDetails")}</button>
              </div>
              <div className="break-all text-text-strong-950">{selected.url}</div>

              {/* What the page sent. */}
              {((selected.requestHeaders && Object.keys(selected.requestHeaders).length > 0) || selected.requestBody) && (
                <div className="flex flex-col gap-1">
                  <div className="text-subheading-2xs uppercase tracking-wide text-text-soft-400">{t("trafficPanel.requestSent")}</div>
                  {selected.requestHeaders && Object.keys(selected.requestHeaders).length > 0 && (
                    <div className="font-mono text-[10px] text-text-sub-600">
                      {Object.entries(selected.requestHeaders).map(([k, v]) => (
                        <div key={k} className="break-all"><span className="text-text-soft-400">{k}:</span> {v}</div>
                      ))}
                    </div>
                  )}
                  {selected.requestBody && (
                    <pre className="m-0 max-h-24 overflow-auto rounded-6 bg-bg-white-0 p-1.5 font-mono text-[10px] text-text-strong-950">{selected.requestBody}</pre>
                  )}
                </div>
              )}

              {/* The real response that came back — read-only, as received. */}
              <div className="flex flex-col gap-1">
                <div className="text-subheading-2xs uppercase tracking-wide text-text-soft-400">{t("trafficPanel.responseReceived")}</div>
                {selected.responseHeaders ? (
                  <>
                    <div className="font-mono text-[10px] text-text-sub-600">
                      <div className="break-all"><span className="text-text-soft-400">{t("trafficPanel.statusLineLabel")}</span> {selected.status ?? "?"}</div>
                      {Object.entries(selected.responseHeaders).map(([k, v]) => (
                        <div key={k} className="break-all"><span className="text-text-soft-400">{k}:</span> {v}</div>
                      ))}
                    </div>
                    <div className="text-subheading-2xs text-text-soft-400">
                      {t("trafficPanel.bodyLabel")}{selected.responseBodyTruncated ? t("trafficPanel.truncatedSuffix") : ""}
                    </div>
                    <pre className="m-0 max-h-40 overflow-auto rounded-6 bg-bg-white-0 p-1.5 font-mono text-[10px] text-text-strong-950">{selected.responseBody || t("trafficPanel.emptyBody")}</pre>
                  </>
                ) : selected.capture === false ? (
                  <div className="text-text-soft-400">{t("trafficPanel.notCaptured", { kind: selected.resource })}</div>
                ) : (
                  <div className="text-text-soft-400">{t("trafficPanel.waitingResponse")}</div>
                )}
              </div>

              {/* Seed a rule from this exact request/response, prefilled. */}
              <div className="flex flex-col gap-1.5 border-t border-stroke-soft-200 pt-1.5">
                <div className="flex flex-wrap items-center gap-1">
                  <span className="mr-0.5 text-[10px] uppercase tracking-wide text-text-soft-400">{t("trafficPanel.addStepToProject")}</span>
                  {([
                    ["traffic.block", t("trafficPanel.quickBlock")],
                    ["traffic.redirect", t("trafficPanel.quickRedirect")],
                    ["traffic.setHeaders", t("trafficPanel.quickRewriteRequest")],
                    ["traffic.editResponse", t("trafficPanel.quickEditResponse")],
                    ["traffic.fulfill", t("trafficPanel.quickFakeResponse")],
                  ] as const).map(([kind, label]) => (
                    <Button key={kind} size="xsmall" mode="stroke" disabled={!onAddStep} onClick={() => addStepFromRequest(selected, kind)}>{label}</Button>
                  ))}
                </div>
                <div className="flex flex-wrap items-center gap-1">
                  <span className="mr-0.5 text-[10px] uppercase tracking-wide text-text-soft-400">{t("trafficPanel.mountLiveTest")}</span>
                  {(["modify", "fulfill", "block"] as const).map((t) => (
                    <Button key={"l-" + t} size="xsmall" disabled={!live} onClick={() => mountLiveFromRequest(selected, t)}>{t}</Button>
                  ))}
                </div>
              </div>
            </div>
          )}
        </>
      )}

      {/* request browser (await) */}
      <div className="mt-2 flex items-center gap-2">
        <div className="text-label-sm text-text-strong-950">{t("trafficPanel.pausedTitle")}</div>
        {paused.length > 0 && <span className="text-paragraph-xs text-text-soft-400">{paused.length}</span>}
      </div>
      {paused.length === 0 && (
        <p className="m-0 py-4 text-center text-paragraph-xs text-text-soft-400">
          {t("trafficPanel.pausedEmptyPart1")}<b>{t("trafficPanel.pausedEmptyAwait")}</b>{t("trafficPanel.pausedEmptyPart2")}
        </p>
      )}
      {paused.map((p) => (
        <div key={p.requestId} className="flex flex-col gap-2 rounded-8 bg-bg-weak-50 p-2.5">
          <div className="break-all text-paragraph-xs text-text-strong-950">
            <span className="font-mono text-text-soft-400">{p.request.method}</span> {p.request.url}
          </div>
          <div className="flex flex-wrap gap-2">
            <Button size="xsmall" onClick={() => resolve(p)}>{t("trafficPanel.continueButton")}</Button>
            <Button size="xsmall" mode="stroke" onClick={() => resolve(p, { type: "block", blockReason: "BlockedByClient" })}>{t("trafficPanel.blockButton")}</Button>
            <Button size="xsmall" mode="stroke" onClick={() => resolve(p, { type: "fulfill", status: 200, responseBody: "" })}>{t("trafficPanel.fulfill200Button")}</Button>
          </div>
        </div>
      ))}

      {menu && (
        <div
          className="fixed z-50 min-w-[180px] rounded-8 bg-bg-white-0 py-1 shadow-[var(--shadow-md)] ring-1 ring-stroke-soft-200"
          style={{ left: menu.x, top: menu.y }}
          onClick={(e) => e.stopPropagation()}
        >
          <div className="truncate px-3 py-1 text-[10px] text-text-soft-400" title={menu.item.url}>{menu.item.url}</div>
          <div className="px-3 py-1 text-[10px] uppercase tracking-wide text-text-soft-400">{t("trafficPanel.menuAddStepToProject")}</div>
          {([
            ["traffic.block", t("trafficPanel.menuBlock")],
            ["traffic.redirect", t("trafficPanel.menuRedirect")],
            ["traffic.setHeaders", t("trafficPanel.menuRewriteRequest")],
            ["traffic.editResponse", t("trafficPanel.menuEditResponse")],
            ["traffic.fulfill", t("trafficPanel.menuFakeResponse")],
          ] as const).map(([kind, label]) => (
            <button
              key={"p-" + kind}
              className="block w-full px-3 py-1.5 text-left text-paragraph-sm text-text-strong-950 hover:bg-bg-weak-50 disabled:opacity-40"
              disabled={!onAddStep}
              onClick={() => addStepFromRequest(menu.item, kind)}
            >
              {label}
            </button>
          ))}
          <div className="mt-1 border-t border-stroke-soft-200 px-3 py-1 text-[10px] uppercase tracking-wide text-text-soft-400">{t("trafficPanel.menuMountLiveTest")}</div>
          {(["modify", "fulfill", "block"] as const).map((t) => (
            <button
              key={"m-" + t}
              className="block w-full px-3 py-1.5 text-left text-paragraph-sm text-primary-base hover:bg-bg-weak-50"
              onClick={() => mountLiveFromRequest(menu.item, t)}
            >
              {t}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
