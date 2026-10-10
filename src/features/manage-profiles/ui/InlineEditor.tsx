import { useEffect, useMemo, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { invoke } from "@tauri-apps/api/core";
import { Button, SegmentControl, Switch, Textarea } from "@proxyshard/shardx-ui-kit";
import { Field } from "../../../shared/ui/Field";
import { NumField } from "../../../shared/ui/NumField";
import { Pair } from "../../../shared/ui/Pair";
import { PortList } from "../../../shared/ui/PortList";
import { CSSelect } from "../../../shared/ui/CSSelect";
import { SelectField } from "../../../shared/ui/SelectField";
import { ColorSwatches } from "../../../shared/ui/ColorSwatches";
import { ExtensionPicker } from "./ExtensionPicker";
import { ProxySelect } from "./ProxySelect";
import { HOST_OS } from "../../../shared/lib/utils";
import {
  AUTO_TZ, AUTO_LANG, TIMEZONES, LOCALES, tzOffsetMinutes, tzOffsetLabel,
  MEMORY_OPTIONS, CPU_OPTIONS, MEDIA_COUNT_OPTIONS, REFRESH_RATE_OPTIONS,
  SCREEN_RESOLUTIONS,
  OS_OPTIONS, matchesOs, osIdFor,
} from "../../../shared/constants";
import type { ProfileForm, GeoMode, WebRtcMode } from "../../../entities/profile";
import type { FingerprintEntry } from "../../../entities/fingerprint";
import { fingerprintGet } from "../../../entities/fingerprint";
import type { ProxyEntry } from "../../../entities/proxy";
import { enrichPicksForPreset, claimsMobile } from "../../../entities/profile";
import { useGpuCompat } from "../../../shared/model/gpuCompat";
import { IncompatibleWarningModal } from "../../gpu-compat";
import { useT } from "../../../shared/i18n";
import { useFolderChoices } from "../../../entities/profile/lib/useFolderChoices";

// The monitor this launcher is on, in CSS pixels. Null while it is being asked
// and on a machine with none, and the editor then offers every resolution.
function useHostScreen(): [number, number] | null {
  const [size, setSize] = useState<[number, number] | null>(null);
  useEffect(() => {
    invoke<[number, number] | null>("host_screen")
      .then((v) => setSize(v ?? null))
      .catch(() => setSize(null));
  }, []);
  return size;
}

/** One group of related fields; every group in the editor looks the same. */
function Card({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section className="flex flex-col gap-3 rounded-xl bg-bg-white-0 p-4 shadow-[var(--shadow-xs)] ring-1 ring-inset ring-stroke-soft-200">
      <h4 className="m-0 text-label-sm text-text-strong-950">{title}</h4>
      {children}
    </section>
  );
}

const Hint = ({ children }: { children: React.ReactNode }) => (
  <p className="m-0 -mt-1.5 text-paragraph-xs text-text-soft-400">{children}</p>
);

export function InlineEditor({
  draft, setDraft, proxies, fingerprints, onSave, onCancel,
}: {
  draft: ProfileForm;
  setDraft: (f: ProfileForm) => void;
  proxies: ProxyEntry[];
  fingerprints: FingerprintEntry[];
  onSave: () => void;
  onCancel: () => void;
}) {
  const t = useT();
  const f = draft;
  const folderChoices = useFolderChoices(f.folder);
  const u = <K extends keyof ProfileForm>(k: K, v: ProfileForm[K]) => setDraft({ ...f, [k]: v });

  // OS filter init from bound fingerprint's platform; new profile uses host OS.
  const currentFp = fingerprints.find((x) => x.id === f.gpu_preset_id);
  const [osFilter, setOsFilter] = useState<string>(
    currentFp ? osIdFor(currentFp.platform as string) : HOST_OS
  );
  const gpusForOs = useMemo(
    () => fingerprints.filter((fp) => matchesOs(fp.platform, osFilter)),
    [fingerprints, osFilter],
  );

  // "auto" first, then zones from west to east so a region is easy to find by offset.
  const tzOptions = useMemo(
    () => [
      AUTO_TZ,
      ...TIMEZONES.filter((z) => z !== AUTO_TZ).sort(
        (a, b) => tzOffsetMinutes(a) - tzOffsetMinutes(b) || a.localeCompare(b),
      ),
    ],
    [],
  );

  const hostScreen = useHostScreen();
  // Never larger than this machine's monitor: the window would be clamped to it
  // at launch anyway, and a screen wider than the window it contains is the
  // kind of disagreement a page gets for free.  "" is the template's own.
  const resolutionOptions = useMemo(() => {
    const fits = SCREEN_RESOLUTIONS.filter(
      ([w, h]) => !hostScreen || (w <= hostScreen[0] && h <= hostScreen[1]),
    );
    return ["", ...fits.map(([w, h]) => `${w}x${h}`)];
  }, [hostScreen]);

  /// Pick GPU = full fingerprint snap (fetched lazily); toStored re-fetches
  /// the payload at save time since the bulk list doesn't carry it.
  // The verdict map and the "stop warning me" flag; the load is once per app run.
  const compatById = useGpuCompat((s) => s.byId);
  const suppressed = useGpuCompat((s) => s.suppressed);
  const loadCompat = useGpuCompat((s) => s.load);
  useEffect(() => { void loadCompat(); }, [loadCompat]);
  // A pick held back until the operator has seen what it costs.
  const [pendingGpu, setPendingGpu] = useState<string | null>(null);

  const setGpu = async (id: string) => {
    if (!fingerprints.some((x) => x.id === id)) return;
    // The bulk `fingerprints` list carries no payload (kept light for speed
    // with large libraries) — fetch this one entry's full config directly.
    const fp = await fingerprintGet(id);
    const nav = fp?.payload?.navigator ?? {};
    // Ask Rust for the same hw + platform_version triplet save uses.
    let picks: { hardware_concurrency?: number; device_memory?: number; platform_version?: string } = {};
    try {
      picks = await enrichPicksForPreset(id);
    } catch {
      // Fall back to preset's nav defaults if Rust enrich fails.
    }
    setDraft({
      ...f,
      gpu_preset_id: id,
      hardware_concurrency: picks.hardware_concurrency ?? nav.hardware_concurrency ?? f.hardware_concurrency,
      device_memory: picks.device_memory ?? nav.device_memory ?? f.device_memory,
      platform_version: picks.platform_version ?? f.platform_version,
      user_agent: nav.user_agent ?? f.user_agent,
    });
  };

  // Snap unknown / empty gpu_preset_id to a random GPU of the active OS.
  // Only from fingerprints this machine can back: otherwise the page reads the
  // extension in getSupportedExtensions() and null from getExtension().
  useEffect(() => {
    if (fingerprints.length === 0) return;
    const exists = fingerprints.some((g) => g.id === f.gpu_preset_id);
    if (!exists) {
      const base = gpusForOs.length > 0 ? gpusForOs : fingerprints;
      const fitting = base.filter((g) => compatById[g.id]?.compatible !== false);
      const pool = fitting.length > 0 ? fitting : base;
      const pick = pool[Math.floor(Math.random() * pool.length)];
      if (pick) setGpu(pick.id);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [fingerprints, osFilter, f.gpu_preset_id]);

  /// What the GPU select calls: applies a pick that fits this machine, and asks
  /// first otherwise — the choice stays the operator's, the cost has to be known.
  const chooseGpu = (id: string) => {
    const verdict = compatById[id];
    if (verdict && !verdict.compatible && !suppressed) {
      setPendingGpu(id);
      return;
    }
    void setGpu(id);
  };

  const pickOs = (os: string) => {
    setOsFilter(os);
    // Switch GPU to first of new OS if current doesn't match.
    if (currentFp && !matchesOs(currentFp.platform, os)) {
      const first = fingerprints.find((g) => matchesOs(g.platform, os));
      if (first) setGpu(first.id);
    }
  };

  return (
    <div className="inline-editor relative border-t border-stroke-soft-200 bg-bg-weak-50 px-[18px] py-4 pl-[22px]">
      <div className="absolute left-0 top-0 h-full w-[3px] bg-primary-base" />
      {/* The buttons are at the top and stay there while the form scrolls: a person who has filled
          the page in should not have to scroll to the bottom to find "Save". */}
      <div className="sticky top-0 z-10 -mx-[18px] -mt-4 mb-4 flex items-center justify-between gap-3 border-b border-stroke-soft-200 bg-bg-weak-50/95 px-[18px] py-2.5 pl-[22px] backdrop-blur">
        <span className="min-w-0 truncate text-label-md text-text-strong-950">
          {f.id ? `${t("inlineEditor.saveChanges")} · ${f.name || ""}` : t("inlineEditor.createProfile")}
        </span>
        <div className="flex flex-none gap-2.5">
          <Button variant="neutral" mode="stroke" size="small" onClick={onCancel}>{t("inlineEditor.cancel")}</Button>
          <Button variant="primary" mode="filled" size="small" onClick={onSave}>
            {f.id ? t("inlineEditor.saveChanges") : t("inlineEditor.createProfile")}
          </Button>
        </div>
      </div>
      <div className="grid grid-cols-1 items-stretch gap-4 lg:grid-cols-2 xl:grid-cols-3">
        <div className="flex flex-col gap-4 [&>section:last-child]:flex-1">
          <Card title={t("inlineEditor.cardInfo")}>
            <Field label={t("inlineEditor.nameLabel")} value={f.name} onChange={(v) => u("name", v)} placeholder={t("inlineEditor.namePlaceholder")} />
            <CSSelect
              title={t("inlineEditor.folderLabel")}
              value={f.folder}
              onChange={(v) => u("folder", v)}
              options={[{ value: "", label: t("inlineEditor.folderNone") }, ...folderChoices.map((x) => ({ value: x, label: x }))]}
            />
            <ColorSwatches label={t("inlineEditor.colorLabel")} value={f.color} onChange={(v) => u("color", v)} />
            <Textarea
              label={t("inlineEditor.notesLabel")}
              rows={2}
              value={f.notes}
              onChange={(e) => u("notes", e.target.value)}
              placeholder={t("inlineEditor.notesPlaceholder")}
            />
          </Card>

          <Card title={t("inlineEditor.cardDevice")}>
            <label className="flex flex-col gap-1">
              <span className="text-label-base font-medium text-text-strong-900">{t("inlineEditor.osLabel")}</span>
              <SegmentControl
                size="small"
                className="w-full *:flex-1"
                value={osFilter}
                items={OS_OPTIONS.map((o) => ({ value: o.id, label: o.label }))}
                onChange={pickOs}
              />
            </label>
            <CSSelect
              value={f.gpu_preset_id}
              onChange={(v) => chooseGpu(v)}
              title={t("inlineEditor.gpuLabel")}
              placeholder={t("inlineEditor.gpuEmpty", { os: osFilter })}
              options={gpusForOs.map((g) => ({ value: g.id, label: g.label }))}
            />
            <Field label={t("inlineEditor.userAgentLabel")} value={f.user_agent} onChange={(v) => u("user_agent", v)} mono />
          </Card>

          <Card title={t("inlineEditor.extensionsHeading")}>
            <ExtensionPicker value={f.extensions} onChange={(v) => u("extensions", v)} />
          </Card>
        </div>
        <div className="flex flex-col gap-4 [&>section:last-child]:flex-1">
          <Card title={t("inlineEditor.cardNetwork")}>
            <label className="flex flex-col gap-1">
              <span className="text-label-base font-medium text-text-strong-900">{t("inlineEditor.proxyLabel")}</span>
              <ProxySelect value={f.proxy_id} proxies={proxies} onChange={(id) => u("proxy_id", id)} />
            </label>
            <div className="grid grid-cols-2 gap-3">
              <CSSelect
                value={f.timezone}
                title={t("inlineEditor.timezoneLabel")}
                onChange={(v) => u("timezone", v)}
                options={tzOptions.map((tz) => ({
                  value: tz,
                  label: tz === AUTO_TZ ? t("inlineEditor.timezoneAuto") : `(${tzOffsetLabel(tz)}) ${tz}`,
                }))}
              />
              <CSSelect
                title={t("inlineEditor.languageLabel")}
                value={f.language}
                onChange={(v) => u("language", v)}
                options={LOCALES.map((l) => ({
                  value: l.code,
                  // Every other entry is the language's own name for itself.
                  label: l.code === AUTO_LANG ? t("inlineEditor.languageAuto") : l.label,
                }))}
              />
            </div>
            <label className="flex flex-col gap-1">
              <span className="text-label-base font-medium text-text-strong-900">{t("inlineEditor.geoLabel")}</span>
              <SegmentControl
                size="small"
                className="w-full *:flex-1"
                value={f.geo_mode}
                items={(["auto", "manual"] as GeoMode[]).map((m) => ({
                  value: m,
                  label: m === "auto" ? t("inlineEditor.geoAuto") : t("inlineEditor.geoManual"),
                }))}
                onChange={(v) => u("geo_mode", v as GeoMode)}
              />
            </label>
            {f.geo_mode === "manual" && (
              <div className="grid grid-cols-3 gap-3">
                <NumField label={t("inlineEditor.latitudeLabel")} value={f.geo_lat} onChange={(v) => u("geo_lat", v)} step={0.0001} />
                <NumField label={t("inlineEditor.longitudeLabel")} value={f.geo_lng} onChange={(v) => u("geo_lng", v)} step={0.0001} />
                <NumField label={t("inlineEditor.accuracyLabel")} value={f.geo_accuracy} onChange={(v) => u("geo_accuracy", v)} />
              </div>
            )}
            <div className="grid grid-cols-2 gap-3">
              <CSSelect
                title={t("inlineEditor.webrtcLabel")}
                value={f.webrtc}
                onChange={(v) => u("webrtc", v as WebRtcMode)}
                options={[
                  { value: "auto", label: t("inlineEditor.webrtcAuto") },
                  { value: "tcp_only", label: t("inlineEditor.webrtcTcpOnly") },
                  { value: "block", label: t("inlineEditor.webrtcBlock") },
                ]}
              />
              <CSSelect
                title={t("inlineEditor.dntLabel")}
                value={f.do_not_track ? "1" : "0"}
                onChange={(v) => u("do_not_track", v === "1")}
                options={[
                  { value: "0", label: t("inlineEditor.dntOff") },
                  { value: "1", label: t("inlineEditor.dntOn") },
                ]}
              />
            </div>
          </Card>

          <Card title={t("inlineEditor.noiseHeading")}>
            <div className="grid grid-cols-2 gap-2 gap-x-3">
              <Pair label={t("inlineEditor.noiseCanvas")}        value={f.noise_canvas}        on={(v) => u("noise_canvas", v)} />
              <Pair label={t("inlineEditor.noiseWebgl")}         value={f.noise_webgl}         on={(v) => u("noise_webgl", v)} />
              <Pair label={t("inlineEditor.noiseAudio")}         value={f.noise_audio}         on={(v) => u("noise_audio", v)} />
              <Pair label={t("inlineEditor.noiseClientRects")}   value={f.noise_client_rects}  on={(v) => u("noise_client_rects", v)} />
              <Pair label={t("inlineEditor.noiseSensors")}       value={f.noise_sensors}       on={(v) => u("noise_sensors", v)} />
              <Pair label={t("inlineEditor.noiseFonts")}         value={f.noise_fonts}         on={(v) => u("noise_fonts", v)} onText={t("inlineEditor.noiseFontsOnText")} />
            </div>
          </Card>
        </div>
        <div className="flex flex-col gap-4 [&>section:last-child]:flex-1">
          <Card title={t("inlineEditor.cardHardware")}>
            <div className="grid grid-cols-2 gap-3">
              <SelectField label={t("inlineEditor.cpuLabel")} value={f.hardware_concurrency} onChange={(v) => u("hardware_concurrency", v)} options={CPU_OPTIONS} />
              <SelectField label={t("inlineEditor.memoryLabel")} value={f.device_memory} onChange={(v) => u("device_memory", v)} options={MEMORY_OPTIONS} />
            </div>
            <SelectField
              label={t("inlineEditor.refreshRateLabel")}
              value={f.refresh_rate}
              onChange={(v) => u("refresh_rate", v)}
              options={REFRESH_RATE_OPTIONS}
              format={(v) => `${v} Hz`}
            />
            <Hint>{t("inlineEditor.refreshRateHelp")}</Hint>
            {/* Windows and Linux only: on macOS the profile's own screen is kept
                as it is, so there is nothing here for an operator to choose. */}
            {(osFilter === "Windows" || osFilter === "Linux") && (
              <>
                <SelectField
                  label={t("inlineEditor.resolutionLabel")}
                  value={f.screen_w > 0 ? `${f.screen_w}x${f.screen_h}` : ""}
                  onChange={(v) => {
                    const [w, h] = v ? v.split("x").map(Number) : [0, 0];
                    u("screen_w", w);
                    u("screen_h", h);
                  }}
                  options={resolutionOptions}
                  format={(v) => (v ? String(v).replace("x", " × ") : t("inlineEditor.resolutionTemplate"))}
                />
                <Hint>
                  {hostScreen
                    ? t("inlineEditor.resolutionHelp", { w: hostScreen[0], h: hostScreen[1] })
                    : t("inlineEditor.resolutionHelpNoHost")}
                </Hint>
              </>
            )}
          </Card>

          <Card title={t("inlineEditor.cardPorts")}>
            <PortList label={t("inlineEditor.blockedPortsLabel")} value={f.blocked_ports} onChange={(v) => u("blocked_ports", v)} />
          </Card>

          <Card title={t("inlineEditor.mediaDevicesHeading")}>
            <div className="grid grid-cols-3 gap-3">
              <SelectField label={t("inlineEditor.micLabel")} value={f.media_audio_in} onChange={(v) => u("media_audio_in", v)} options={MEDIA_COUNT_OPTIONS} />
              <SelectField label={t("inlineEditor.speakersLabel")} value={f.media_audio_out} onChange={(v) => u("media_audio_out", v)} options={MEDIA_COUNT_OPTIONS} />
              <SelectField label={t("inlineEditor.webcamLabel")} value={f.media_video_in} onChange={(v) => u("media_video_in", v)} options={MEDIA_COUNT_OPTIONS} />
            </div>
            {claimsMobile(f) && (
              <>
                <Switch label={t("inlineEditor.androidMediaLabel")} checked={f.android_media} onChange={(checked) => u("android_media", checked)} />
                <Hint>{t("inlineEditor.androidMediaHelp")}</Hint>
              </>
            )}
          </Card>

          <Card title={t("inlineEditor.cookiesHeading")}>
            <div className="flex items-center gap-2">
              <Button
                variant="neutral"
                mode="stroke"
                size="2xsmall"
                onClick={async () => {
                  const path = await open({
                    multiple: false, directory: false, title: t("inlineEditor.cookiesDialogTitle"),
                    filters: [{ name: "Cookies", extensions: ["json", "txt"] }],
                  });
                  if (typeof path === "string") u("cookies_file", path);
                }}
              >
                {f.cookies_file ? t("inlineEditor.cookiesChange") : t("inlineEditor.cookiesLoad")}
              </Button>
              {f.cookies_file && (
                <>
                  <span className="truncate text-paragraph-xs text-text-sub-600" title={f.cookies_file}>
                    {f.cookies_file.split(/[/\\]/).pop()}
                  </span>
                  <button type="button" className="text-paragraph-xs text-text-soft-400 hover:text-error-base" onClick={() => u("cookies_file", "")}>
                    {t("inlineEditor.cookiesRemove")}
                  </button>
                </>
              )}
            </div>
            <Hint>{t("inlineEditor.cookiesHelp")}</Hint>
          </Card>
        </div>
      </div>
      {pendingGpu && compatById[pendingGpu] && (
        <IncompatibleWarningModal
          compat={compatById[pendingGpu]}
          onKeep={() => {
            const id = pendingGpu;
            setPendingGpu(null);
            void setGpu(id);
          }}
          onCancel={() => setPendingGpu(null)}
        />
      )}
    </div>
  );
}
