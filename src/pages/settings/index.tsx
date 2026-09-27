import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import { Button, Input, Select, Switch, Textarea } from "@proxyshard/shardx-ui-kit";
import { DownloadIcon } from "../../shared/icons";
import { Topbar } from "../../shared/ui/Topbar";
import { CopyField } from "../../shared/ui/CopyField";
import { toast } from "../../shared/model/toast";
import { withUtm } from "../../shared/lib/utils";
import type { Settings, ApiInfo } from "../../entities/settings";
import { HELPER_KINDS } from "../../entities/settings";
import { settingsGet, settingsSave, settingsLoadError, apiInfo, apiRegenerateToken, mcpDownload } from "../../entities/settings";
import { DataRootCard } from "../../features/manage-profiles/ui/DataRootCard";
import { useT, useLang, LANG_OPTIONS, type Lang } from "../../shared/i18n";
import type { LicenseInfo } from "../../entities/license";
import { licenseInfo } from "../../entities/license";
import { Section, Row, Block } from "./ui";
import { TeamTab } from "./TeamTab";
import { useTeam } from "../../shared/model/teamRole";
import { FingerprintPanel } from "./FingerprintPanel";

type Tab = "general" | "team" | "advanced" | "license";

export function SettingsPage() {
  const t = useT();
  const lang = useLang((st) => st.lang);
  const setLang = useLang((st) => st.setLang);
  const [tab, setTab] = useState<Tab>("general");
  const [s, setS] = useState<Settings>({
    browser_path: null,
    theme: "dark",
    geo_checker: "ip-api.com",
    screen_resolution_mode: "fingerprint",
    helper_enabled: true,
    helper_triggers: [],
    extra_args: "",
    api_enabled: true,
    api_port: 40325,
  });
  const [saved, setSaved] = useState("");
  const [api, setApi] = useState<ApiInfo | null>(null);
  const refreshApi = () => apiInfo().then(setApi).catch(() => {});
  const [loadError, setLoadError] = useState<string | null>(null);
  useEffect(() => {
    settingsGet().then((v) => { setS(v); setSaved(JSON.stringify(v)); });
    settingsLoadError().then(setLoadError).catch(() => {});
    refreshApi();
  }, []);
  const dirty = saved !== "" && JSON.stringify(s) !== saved;

  const [license, setLicense] = useState<LicenseInfo | null>(null);
  useEffect(() => { licenseInfo().then(setLicense).catch(() => {}); }, []);
  const regenToken = async () => {
    try { setApi(await apiRegenerateToken()); toast.ok(t("settings.tokenRegenerated")); }
    catch (e) { toast.err(String(e)); }
  };

  const [mcpBusy, setMcpBusy] = useState(false);
  // Download MCP server source; user manages install + client setup.
  const downloadMcp = async () => {
    const dir = await open({ directory: true, title: t("settings.mcpDownloadDialogTitle") });
    if (typeof dir !== "string") return;
    setMcpBusy(true);
    try {
      const path = await mcpDownload(dir);
      toast.ok(t("settings.mcpDownloaded", { path }));
    } catch (e) { toast.err(t("settings.mcpDownloadFailed", { err: String(e) })); }
    finally { setMcpBusy(false); }
  };
  const save = async () => {
    try { await settingsSave(s); setSaved(JSON.stringify(s)); toast.ok(t("settings.saved")); refreshApi(); }
    catch (e) { toast.err(String(e)); }
  };

  // Disconnecting is security-sensitive (it's how you invalidate a leaked admin
  // code) — it must not sit as an "unsaved change" waiting for a Lưu cài đặt
  // click. Persist it at once and drop the cached role right away, or the
  // Nhân sự list (built from the old, still-saved token) keeps showing.
  const disconnect = async () => {
    const next = { ...s, sync: { ...s.sync!, enabled: false, server_url: null, token: null } };
    setS(next);
    try { await settingsSave(next); setSaved(JSON.stringify(next)); }
    catch (e) { toast.err(String(e)); return; }
    useTeam.setState({ role: null, name: "", id: "", folderNames: null });
    try { localStorage.removeItem("hir.teamRole"); } catch { /* ignore */ }
    toast.ok("Đã ngắt kết nối");
  };

  const TABS: { id: Tab; label: string }[] = [
    { id: "general", label: "Chung" },
    { id: "team", label: t("settings.syncTitle") },
    { id: "advanced", label: "Nâng cao" },
    ...(license ? [{ id: "license" as Tab, label: t("settings.licenseTitle") }] : []),
  ];
  const picked = s.helper_triggers ?? [];

  return (
    <section className="flex flex-col">
      <Topbar crumbs={[t("settings.crumbSystem"), t("settings.crumbSettings")]} search="" onSearch={() => {}} />
      <h1 className="m-0 mb-4 text-title-h5 text-text-strong-950">{t("settings.title")}</h1>

      <nav className="mb-4 flex gap-1 border-b border-stroke-soft-200">
        {TABS.map((x) => (
          <button
            key={x.id}
            type="button"
            onClick={() => setTab(x.id)}
            className={`-mb-px cursor-pointer border-0 border-b-2 bg-transparent px-4 py-2.5 text-label-sm transition-colors ${
              tab === x.id ? "border-b-primary-base text-text-strong-950" : "border-b-transparent text-text-sub-600 hover:text-text-strong-950"
            }`}
          >
            {x.label}
          </button>
        ))}
      </nav>

      {loadError && (
        <div className="mb-4 rounded-xl bg-bg-white-0 p-[18px] text-paragraph-sm text-text-strong-950 ring-1 ring-inset ring-error-base">
          <strong>{t("settings.loadErrorTitle")}</strong>{t("settings.loadErrorBody1")}<code>settings.json.bad</code>{t("settings.loadErrorBody2")}<code>Set-Content -Encoding UTF8</code>{t("settings.loadErrorBody3")}
          <div className="mt-1 text-paragraph-xs text-text-soft-400">{loadError}</div>
        </div>
      )}

      <div className="flex max-w-[980px] flex-col gap-4 pb-6">
        {tab === "general" && (
          <>
            <Section title="Giao diện & hiển thị">
              <Row label={t("settings.languageTitle")} hint={t("settings.languageHelp")}>
                <Select size="small" value={lang} onChange={(v) => setLang(v as Lang)} options={LANG_OPTIONS.map((o) => ({ value: o.value, label: o.label }))} />
              </Row>
              <Row
                label={t("settings.screenTitle")}
                hint={<><strong>{t("settings.screenFromFingerprintWord")}</strong>{t("settings.screenHelp1")}<strong>{t("settings.screenRealWord")}</strong>{t("settings.screenHelp2")}</>}
              >
                <Select
                  size="small"
                  value={s.screen_resolution_mode ?? "fingerprint"}
                  onChange={(v) => setS({ ...s, screen_resolution_mode: v })}
                  options={[
                    { value: "fingerprint", label: t("settings.screenModeFingerprint") },
                    { value: "real", label: t("settings.screenModeReal") },
                  ]}
                />
              </Row>
              <Row label={t("settings.geoCheckerTitle")} hint={<>{t("settings.geoCheckerHelp1")}<strong>{t("settings.geoCheckerTestWord")}</strong>{t("settings.geoCheckerHelp2")}</>}>
                <Select
                  size="small"
                  value={s.geo_checker ?? "ip-api.com"}
                  onChange={(v) => setS({ ...s, geo_checker: v })}
                  options={[
                    { value: "ip-api.com", label: t("settings.geoIpApiCom") },
                    { value: "ipapi.co", label: t("settings.geoIpapiCo") },
                    { value: "ipwho.is", label: t("settings.geoIpwhoIs") },
                  ]}
                />
              </Row>
              <Row label={t("settings.cameraTitle")} hint={<>{t("settings.cameraHelp1")}<strong>{t("settings.cameraLeaveOn")}</strong>{t("settings.cameraHelp2")}</>}>
                <div className="sm:flex sm:justify-end"><Switch checked={s.camera_enabled ?? true} onChange={(c) => setS({ ...s, camera_enabled: c })} /></div>
              </Row>
            </Section>

            <Section
              title={t("settings.helperTitle")}
              desc={<>{t("settings.helperHelp1")}<strong>{t("settings.helperOffersWord")}</strong>{t("settings.helperHelp2")} <strong>{t("settings.helperNeverSync")}</strong>{t("settings.helperHelp3")}</>}
            >
              <Row label={t("settings.helperEnableLabel")}>
                <div className="sm:flex sm:justify-end"><Switch checked={s.helper_enabled ?? true} onChange={(c) => setS({ ...s, helper_enabled: c })} /></div>
              </Row>
              {(s.helper_enabled ?? true) && (
                <Block>
                  <div className="flex flex-col gap-0.5">
                    <span className="text-label-sm text-text-strong-950">{t("settings.helperReactTo")}</span>
                    <span className="text-paragraph-xs text-text-soft-400">{t("settings.helperTriggersHelp")}</span>
                  </div>
                  <div className="flex flex-wrap gap-1.5">
                    {HELPER_KINDS.map((k) => {
                      const on = picked.includes(k.value);
                      return (
                        <button
                          key={k.value}
                          type="button"
                          onClick={() => setS({ ...s, helper_triggers: on ? picked.filter((x) => x !== k.value) : [...picked, k.value] })}
                          className={`rounded-full px-3 py-1 text-paragraph-xs ring-1 ring-inset transition-colors ${
                            on ? "bg-primary-alpha-10 text-primary-base ring-primary-alpha-24" : "bg-transparent text-text-sub-600 ring-stroke-soft-200 hover:bg-bg-weak-50"
                          }`}
                        >
                          {t(k.label)}
                        </button>
                      );
                    })}
                  </div>
                </Block>
              )}
            </Section>
          </>
        )}

        {tab === "team" && (
          <TeamTab
            sync={s.sync}
            onSyncChange={(patch) => setS({ ...s, sync: { ...s.sync!, ...patch } as typeof s.sync })}
            onDisconnect={disconnect}
          />
        )}

        {tab === "advanced" && (
          <>
            <Section title={t("settings.dataLocationTitle")}>
              <Block><DataRootCard /></Block>
            </Section>

            <Section
              title={t("settings.extraArgsTitle")}
              desc={<>{t("settings.extraArgsHelp1")}<strong>{t("settings.extraArgsLastWord")}</strong>{t("settings.extraArgsHelp2")} {t("settings.extraArgsHelp3")}</>}
            >
              <Block>
                <Textarea rows={3} className="mono" value={s.extra_args ?? ""} onChange={(e) => setS({ ...s, extra_args: e.target.value })} placeholder={"--disable-background-timer-throttling\n--window-size=1280,800"} />
              </Block>
            </Section>

            <Section
              title={t("settings.apiTitle")}
              desc={
                <>
                  {t("settings.apiHelp1")}<strong>127.0.0.1</strong>{t("settings.apiHelp2")}{" "}
                  <a href="#" className="text-primary-base hover:underline" onClick={(e) => { e.preventDefault(); openUrl(withUtm("https://docs.proxyshard.com/eng/shardx-launcher-api/binding-and-lifecycle?fallback=true")).catch(() => {}); }}>
                    {t("settings.apiRefLink")}
                  </a>
                </>
              }
            >
              <Row label={t("settings.apiEnableLabel")}>
                <div className="sm:flex sm:justify-end"><Switch checked={s.api_enabled ?? true} onChange={(c) => setS({ ...s, api_enabled: c })} /></div>
              </Row>
              <Row label={t("settings.apiPortLabel")}>
                <Input inputSize="small" type="number" value={s.api_port ?? 40325} onChange={(e) => setS({ ...s, api_port: Number(e.target.value) || 40325 })} />
              </Row>
              {api && (
                <>
                  <Row label={t("settings.apiBaseUrlLabel")}><CopyField value={api.base_url} /></Row>
                  <Row label={t("settings.apiTokenLabel")} hint={<>{t("settings.apiAuthHeaderHint")}<code>Authorization: Bearer &lt;token&gt;</code></>}>
                    <CopyField value={api.token} secret />
                  </Row>
                  <Row label={t("settings.apiRegenerateBtn")} hint={t("settings.apiRegenerateHint")}>
                    <div className="sm:text-right"><Button variant="neutral" mode="stroke" size="small" onClick={regenToken}>{t("settings.apiRegenerateBtn")}</Button></div>
                  </Row>
                </>
              )}
            </Section>

            <FingerprintPanel />

            <Section title={t("settings.mcpTitle")} desc={<>{t("settings.mcpHelp1")}<strong>MCP</strong>{t("settings.mcpHelp2")}</>}>
              <Block>
                <div>
                  <Button variant="neutral" mode="stroke" size="small" leftIcon={<DownloadIcon className="size-4" />} onClick={downloadMcp} disabled={mcpBusy} isLoading={mcpBusy}>
                    {mcpBusy ? t("settings.mcpDownloading") : t("settings.mcpDownloadBtn")}
                  </Button>
                </div>
              </Block>
            </Section>
          </>
        )}

        {tab === "license" && license && (
          <Section title={t("settings.licenseTitle")}>
            <Row label={t("settings.licenseKeyLabel")}><span className="mono text-paragraph-sm text-text-strong-950 sm:block sm:text-right">{license.key}</span></Row>
            <Row label={t("settings.licenseNameLabel")}><span className="text-paragraph-sm text-text-sub-600 sm:block sm:text-right">{license.customer_name ?? "—"}</span></Row>
            <Row label={t("settings.licensePhoneLabel")}><span className="text-paragraph-sm text-text-sub-600 sm:block sm:text-right">{license.customer_phone ?? "—"}</span></Row>
            <Row label={t("settings.licenseEmailLabel")}><span className="text-paragraph-sm text-text-sub-600 sm:block sm:text-right">{license.customer_email ?? "—"}</span></Row>
            <Row label={t("settings.licenseDeviceLabel")}><CopyField value={license.device_id} /></Row>
          </Section>
        )}
      </div>

      {/* Save bar: only there when there is something to save. */}
      {dirty && (
        <div className="sticky bottom-0 z-20 -mx-1 mt-2 flex max-w-[980px] items-center justify-between gap-3 rounded-xl bg-bg-white-0 px-4 py-3 shadow-[var(--shadow-md,0_4px_16px_rgba(0,0,0,0.25))] ring-1 ring-inset ring-stroke-soft-200">
          <span className="text-paragraph-sm text-text-sub-600">Có thay đổi chưa lưu</span>
          <div className="flex gap-2">
            <Button variant="neutral" mode="stroke" size="small" onClick={() => settingsGet().then((v) => { setS(v); setSaved(JSON.stringify(v)); })}>Hoàn tác</Button>
            <Button variant="primary" mode="filled" size="small" onClick={save}>{t("settings.saveBtn")}</Button>
          </div>
        </div>
      )}
    </section>
  );
}
