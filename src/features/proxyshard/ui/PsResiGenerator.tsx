import { useEffect, useState } from "react";
import { DialogModal, SegmentControl, SelectOption, Tooltip } from "@proxyshard/shardx-ui-kit";
import { Field } from "../../../shared/ui/Field";
import { NumField } from "../../../shared/ui/NumField";
import { CSSelect } from "../../../shared/ui/CSSelect";
import { ChevronDownIcon, InfoIcon } from "../../../shared/icons";
import { toast } from "../../../shared/model/toast";
import { randSid } from "../../../shared/lib/utils";
import { useT } from "../../../shared/i18n";
import type { ResiType, PsLoc } from "../../../entities/proxyshard";
import { PS_PLAN, PS_PROXY_TYPE, PS_RELAYS, PS_PORT, psProfileTraffic, psCountries, psRegions, psCities, psResiIsps } from "../../../entities/proxyshard";
import { proxyBulkSave } from "../../../entities/proxy";


const sessionModeOptions = (t: (key: string) => string): SelectOption[] => [
  {
    label: t("psResiGenerator.sessionModeDefaultOption"),
    value: "default"
  },
  {
    label: t("psResiGenerator.sessionModeStaticOption"),
    value: "static"
  }
]

const pofOptions = (t: (key: string) => string): SelectOption[] => [
  {
    label: t("psResiGenerator.osUnset"),
    value: "unset"
  },
  {
    label: "MacOS",
    value: "macos"
  },
  {
    label: "Windows",
    value: "windows"
  },
  {
    label: "Android",
    value: "android"
  },
  {
    label: "Linux",
    value: "linux"
  },
  {
    label: "IOS",
    value: "ios"
  },
]

const PROTO_OPTIONS: SelectOption[] = [
  { value: "http", label: "HTTP" },
  { value: "socks5", label: "SOCKS5" },
];

const sessionOptions = (t: (key: string) => string): SelectOption[] => [
  { value: "rotating", label: t("psResiGenerator.sessionRotating") },
  { value: "sticky", label: t("psResiGenerator.sessionSticky") },
];

export function PsResiGenerator({ type, onClose }: { type: ResiType; onClose: () => void }) {
  const t = useT();
  const plan = PS_PLAN[type];
  const pt = PS_PROXY_TYPE[type];
  const [password, setPassword] = useState("");
  const [pwErr, setPwErr] = useState("");
  const [relay, setRelay] = useState(PS_RELAYS[0]);
  const [proto, setProto] = useState<"http" | "socks5">("socks5");
  const [session, setSession] = useState<"rotating" | "sticky">("sticky");
  const [count, setCount] = useState(1);
  const [prefix, setPrefix] = useState(`${type} resi`);
  const [sessionMode, setSessionMode] = useState<"default" | "static">("default");
  // The OS the exit device should look like. Premium only — the other plans
  // ignore the token, so the field is not offered there and is dropped if the
  // dialog is reopened on a cheaper plan.
  const [pof, setPof] = useState<"unset" | "macos" | "windows" | "android" | "linux" | "ios">("unset");
  const canPickOs = type === "premium";
  const [showAdvanced, setShowAdvanced] = useState(false);


  const [countries, setCountries] = useState<PsLoc[]>([]);
  const [country, setCountry] = useState("");
  const [regions, setRegions] = useState<PsLoc[]>([]);
  const [region, setRegion] = useState("");
  const [cities, setCities] = useState<PsLoc[]>([]);
  const [city, setCity] = useState("");
  const [isps, setIsps] = useState<PsLoc[]>([]);
  const [isp, setIsp] = useState("");
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    psProfileTraffic(pt)
      .then((r) => {
        const p = r.proxy_password ?? r.password ?? "";
        setPassword(p);
        if (!p) setPwErr(t("psResiGenerator.noPasswordError"));
      })
      .catch((e) => setPwErr(String(e)));
    psCountries(pt)
      .then((r) => setCountries(r.results ?? []))
      .catch((e) => toast.err(String(e)));
  }, [pt]);

  // Region depends on country; city depends on region.
  useEffect(() => {
    setRegion(""); setRegions([]); setCity(""); setCities([]); setIsp(""); setIsps([]);
    if (!country) return;
    psRegions(pt, country)
      .then((r) => setRegions(r.results ?? [])).catch(() => { });
  }, [country]); // eslint-disable-line react-hooks/exhaustive-deps
  useEffect(() => {
    setCity(""); setCities([]); setIsp(""); setIsps([]);
    if (!country || !region) return;
    psCities(pt, country, region)
      .then((r) => setCities(r.results ?? [])).catch(() => { });
  }, [region]); // eslint-disable-line react-hooks/exhaustive-deps
  // ISP directory is per-city and premium-only.
  useEffect(() => {
    setIsp(""); setIsps([]);
    if (type !== "premium" || !country || !region || !city) return;
    psResiIsps(pt, country, region, city)
      .then((r) => setIsps(r.results ?? [])).catch(() => { });
  }, [city]); // eslint-disable-line react-hooks/exhaustive-deps

  const buildUser = (sid: string | null) => {
    const parts = [`plan-${plan}`];
    if (country) parts.push(`country-${country.toLowerCase()}`);
    if (region) parts.push(`region-${region}`);
    if (city) parts.push(`city-${city}`);
    if (isp) parts.push(`isp-${isp}`);
    if (sid) parts.push(`sid-${sid}`);
    if (canPickOs && pof !== "unset") parts.push(`os-${pof}`);
    if (sessionMode === "default") parts.push("session_mode-2");
    return parts.join("-");
  };
  const sampleUser = buildUser(session === "sticky" ? "‹sid›" : null);

  const generate = async () => {
    if (!password) { toast.err(t("psResiGenerator.noPasswordToast")); return; }
    const port = PS_PORT[proto];
    const n = Math.max(1, Math.round(count));
    const entries = Array.from({ length: n }, (_, i) => ({
      id: "",
      name: `${prefix.trim() || "resi"}${country ? " " + country.toUpperCase() : ""}${n > 1 ? ` #${i + 1}` : ""}`,
      kind: proto,
      host: relay,
      port,
      username: buildUser(session === "sticky" ? randSid() : null),
      password,
      country: country ? country.toUpperCase() : "",
      notes: `ProxyShard residential (${plan})`,
    }));
    setSaving(true);
    try {
      const added = await proxyBulkSave(entries);
      toast.ok(added > 0
        ? (added === 1 ? t("psResiGenerator.addedOne", { n: added }) : t("psResiGenerator.addedMany", { n: added }))
        : t("psResiGenerator.noNewProxies"));
     // onClose();
    } catch (e) { toast.err(String(e)); }
    finally { setSaving(false); }
  };

  return (
    <DialogModal
      open
      onClose={onClose}
      title={t("psResiGenerator.title", { plan })}
      maxWidthClassName="max-w-[880px]"
      confirmLabel={saving ? t("psResiGenerator.generating") : t("psResiGenerator.generateCount", { n: Math.max(1, Math.round(count)) })}
      onConfirm={generate}
      isLoading={saving}
      isDisabled={saving || !password}
      cancelLabel={t("psResiGenerator.cancel")}
      onCancel={onClose}
    >
      <div className="flex flex-col gap-3 py-4 w-[450px]">
        <div className="grid grid-cols-2 gap-3">
          <CSSelect
            value={relay}
            title={t("psResiGenerator.relayLabel")}
            onChange={setRelay}
            options={PS_RELAYS.map((r) => ({ value: r, label: r }))}
          />

          <label className="flex flex-col gap-1">
            <span className="text-label-base font-medium text-text-strong-900">{t("psResiGenerator.protocolLabel")}</span>
            <SegmentControl
              size="small"
              value={proto}
              items={PROTO_OPTIONS}
              onChange={(v) => setProto(v as "http" | "socks5")}
            />
          </label>
        </div>
        <div className="grid grid-cols-2 gap-3">
          <CSSelect
            value={country}
            title={t("psResiGenerator.countryLabel")}
            onChange={setCountry}
            placeholder={t("psResiGenerator.countryAnyPlaceholder")}
            isSearchable
            searchPlaceholder={t("psResiGenerator.searchCountries")}
            options={[{ value: "", label: t("psResiGenerator.countryAnyOption") }, ...countries.map((c) => ({ value: c.code, label: `${c.name} (${c.code})` }))]}
          />
          <label className="flex flex-col gap-1">
            <span className="text-label-base font-medium text-text-strong-900">{t("psResiGenerator.sessionLabel")}</span>
            <SegmentControl
              size="small"
              value={session}
              items={sessionOptions(t)}
              onChange={(v) => setSession(v as "rotating" | "sticky")}
            />
          </label>
        </div>
        <div className="grid grid-cols-2 gap-3">
          <CSSelect
            value={region}
            title={t("psResiGenerator.regionLabel")}
            onChange={setRegion}
            placeholder={country ? t("psResiGenerator.regionAnyPlaceholder") : t("psResiGenerator.pickCountryFirst")}
            isSearchable
            searchPlaceholder={t("psResiGenerator.searchRegions")}
            options={[{ value: "", label: t("psResiGenerator.regionAnyOption") }, ...regions.map((r) => ({ value: r.code, label: r.name }))]}
          />

          <CSSelect
            value={city}
            title={t("psResiGenerator.cityLabel")}
            onChange={setCity}
            placeholder={region ? t("psResiGenerator.cityAnyPlaceholder") : t("psResiGenerator.pickRegionFirst")}
            isSearchable
            searchPlaceholder={t("psResiGenerator.searchCities")}
            options={[{ value: "", label: t("psResiGenerator.cityAnyOption") }, ...cities.map((c) => ({ value: c.code, label: c.name }))]}
          />
          {
            type === "premium" && (
              <CSSelect
                value={isp}
                title={t("psResiGenerator.ispLabel")}
                onChange={setIsp}
                placeholder={city ? t("psResiGenerator.anyIspPlaceholder") : t("psResiGenerator.pickCityFirst")}
                options={[{ value: "", label: t("psResiGenerator.ispAnyOption") }, ...isps.map((x) => ({ value: x.code, label: x.name }))]}
              />
            )
          }
        </div>
        <div className="grid grid-cols-2 gap-3">
          <Field label={t("psResiGenerator.namePrefixLabel")} value={prefix} onChange={setPrefix} />
          <NumField label={session === "sticky" ? t("psResiGenerator.countStickyLabel") : t("psResiGenerator.countLabel")} value={count} onChange={(v) => setCount(Math.max(1, Math.round(v)))} />
        </div>
        <div className="flex flex-col gap-3">
          {
            type === "premium" && (
              <button
                type="button"
                aria-expanded={showAdvanced}
                onClick={() => setShowAdvanced((v) => !v)}
                className="flex w-fit items-center gap-2 rounded-lg text-label-sm font-medium text-text-soft-400 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary-base"
              >
                {t("psResiGenerator.advancedSettings")}
                <ChevronDownIcon
                  aria-hidden="true"
                  className={`size-4 transition-transform duration-200 ${showAdvanced ? "rotate-180" : ""}`}
                />
              </button>
            )
          }
          {showAdvanced && canPickOs && (
            <div className="flex flex-col gap-1">
              <div className="flex items-center gap-1">
                <span className="text-label-base font-medium text-text-strong-900">{t("psResiGenerator.osLabel")}</span>
                <Tooltip
                  content={t("psResiGenerator.osHelp")}
                  side="top"
                  className="left-20"
                >
                  <InfoIcon className="size-4 cursor-help text-text-soft-400" />
                </Tooltip>
              </div>
              <CSSelect
                value={pof}
                onChange={(v) => setPof(v as typeof pof)}
                options={pofOptions(t)}
              />
            </div>
          )}
          {showAdvanced && (
            <div className="flex flex-col gap-1">
              <div className="flex items-center gap-1">
                <span className="text-label-base font-medium text-text-strong-900">{t("psResiGenerator.sessionModeLabel")}</span>
                <Tooltip
                  content={t("psResiGenerator.sessionModeHelp")}
                  side="top"
                  className="left-20"
                >
                  <InfoIcon className="size-4 cursor-help text-text-soft-400" />
                </Tooltip>
              </div>
              <CSSelect
                value={sessionMode}
                onChange={(v) => setSessionMode(v as "default" | "static")}
                options={sessionModeOptions(t)}
              />
            </div>
          )}
        </div>
        <div className="mono mt-1.5 break-all rounded-8 bg-bg-weak-50 px-[11px] py-[9px] text-paragraph-xs text-text-sub-600 ring-1 ring-inset ring-stroke-soft-200">
          {relay}:{PS_PORT[proto]}:{sampleUser}:{password ? "••••" : "?"}
        </div>
        <span className="text-label-sm font-medium text-text-soft-400">{t("psResiGenerator.note")}</span>
        {pwErr && <p className="m-0 text-paragraph-xs text-text-soft-400">{pwErr}</p>}
      </div>
    </DialogModal>
  );
}
