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

function SettingsCard({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div className="mb-3.5 rounded-lg bg-bg-white-0 p-[18px] shadow-[var(--shadow-xs)] ring-1 ring-inset ring-stroke-soft-200">
      <h3 className="m-0 mb-1.5 text-label-sm text-text-strong-950">{title}</h3>
      {children}
    </div>
  );
}

export function SettingsPage() {
  const t = useT();
  const lang = useLang((st) => st.lang);
  const setLang = useLang((st) => st.setLang);
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
  const [api, setApi] = useState<ApiInfo | null>(null);
  const refreshApi = () => apiInfo().then(setApi).catch(() => {});
  const [loadError, setLoadError] = useState<string | null>(null);
  useEffect(() => {
    settingsGet().then(setS);
    settingsLoadError().then(setLoadError).catch(() => {});
    refreshApi();
  }, []);
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
    try { await settingsSave(s); toast.ok(t("settings.saved")); }
    catch (e) { toast.err(String(e)); }
  };
  return (
    <section className="flex flex-col">
      <Topbar crumbs={[t("settings.crumbSystem"), t("settings.crumbSettings")]} search="" onSearch={() => {}} />
      <div className="mb-3.5 flex items-end justify-between gap-4">
        <h1 className="m-0 text-title-h5 text-text-strong-950">{t("settings.title")}</h1>
      </div>

      {loadError && (
        <div className="mb-3.5 rounded-lg bg-bg-white-0 p-[18px] text-paragraph-sm text-text-strong-950 shadow-[var(--shadow-xs)] ring-1 ring-inset ring-error-base">
          <strong>{t("settings.loadErrorTitle")}</strong>{t("settings.loadErrorBody1")}<code>settings.json.bad</code>{t("settings.loadErrorBody2")}<code>Set-Content -Encoding UTF8</code>{t("settings.loadErrorBody3")}
          <div className="mt-1 text-paragraph-xs text-text-soft-400">{loadError}</div>
        </div>
      )}

      <SettingsCard title={t("settings.languageTitle")}>
        <p className="m-0 mb-2 text-paragraph-xs text-text-soft-400">
          {t("settings.languageHelp")}
        </p>
        <Select
          label={t("settings.interfaceLanguageLabel")}
          size="small"
          value={lang}
          onChange={(v) => setLang(v as Lang)}
          options={LANG_OPTIONS.map((o) => ({ value: o.value, label: o.label }))}
        />
      </SettingsCard>

      <SettingsCard title={t("settings.geoCheckerTitle")}>
        <p className="m-0 mb-2 text-paragraph-xs text-text-soft-400">
          {t("settings.geoCheckerHelp1")}<strong>{t("settings.geoCheckerTestWord")}</strong>{t("settings.geoCheckerHelp2")}
        </p>
        <Select
          label={t("settings.providerLabel")}
          size="small"
          value={s.geo_checker ?? "ip-api.com"}
          onChange={(v) => setS({ ...s, geo_checker: v })}
          options={[
            { value: "ip-api.com", label: t("settings.geoIpApiCom") },
            { value: "ipapi.co", label: t("settings.geoIpapiCo") },
            { value: "ipwho.is", label: t("settings.geoIpwhoIs") },
          ]}
        />
      </SettingsCard>

      <SettingsCard title={t("settings.screenTitle")}>
        <p className="m-0 mb-2 text-paragraph-xs text-text-soft-400">
          <strong>{t("settings.screenFromFingerprintWord")}</strong>{t("settings.screenHelp1")}
          <strong>{t("settings.screenRealWord")}</strong>{t("settings.screenHelp2")}
        </p>
        <Select
          label={t("settings.screenModeLabel")}
          size="small"
          value={s.screen_resolution_mode ?? "fingerprint"}
          onChange={(v) => setS({ ...s, screen_resolution_mode: v })}
          options={[
            { value: "fingerprint", label: t("settings.screenModeFingerprint") },
            { value: "real", label: t("settings.screenModeReal") },
          ]}
        />
      </SettingsCard>

      <SettingsCard title={t("settings.helperTitle")}>
        <p className="m-0 mb-2 text-paragraph-xs text-text-soft-400">
          {t("settings.helperHelp1")}
          <strong>{t("settings.helperOffersWord")}</strong>{t("settings.helperHelp2")}
          <br />
          <strong>{t("settings.helperNeverSync")}</strong>{t("settings.helperHelp3")}
        </p>
        <div className="flex flex-col gap-3">
          <Switch
            label={t("settings.helperEnableLabel")}
            checked={s.helper_enabled ?? true}
            onChange={(checked) => setS({ ...s, helper_enabled: checked })}
          />
          {(s.helper_enabled ?? true) && (
            <div>
              <div className="mb-1.5 text-label-xs text-text-sub-600">
                {t("settings.helperReactTo")}
              </div>
              <p className="m-0 mb-2 text-paragraph-xs text-text-soft-400">
                {t("settings.helperTriggersHelp")}
              </p>
              <div className="flex flex-wrap gap-1.5">
                {HELPER_KINDS.map((k) => {
                  const picked = (s.helper_triggers ?? []).includes(k.value);
                  return (
                    <button
                      key={k.value}
                      type="button"
                      onClick={() => {
                        const cur = s.helper_triggers ?? [];
                        setS({
                          ...s,
                          helper_triggers: picked
                            ? cur.filter((x) => x !== k.value)
                            : [...cur, k.value],
                        });
                      }}
                      className={`rounded-6 px-2 py-1 text-paragraph-xs ring-1 ring-inset transition-colors ${
                        picked
                          ? "bg-primary-alpha-10 text-primary-base ring-primary-alpha-24"
                          : "text-text-sub-600 ring-stroke-soft-200 hover:bg-bg-weak-50"
                      }`}
                    >
                      {t(k.label)}
                    </button>
                  );
                })}
              </div>
            </div>
          )}
        </div>
      </SettingsCard>

      <SettingsCard title={t("settings.cameraTitle")}>
        <p className="m-0 mb-2 text-paragraph-xs text-text-soft-400">
          {t("settings.cameraHelp1")}
          <strong>{t("settings.cameraLeaveOn")}</strong>{t("settings.cameraHelp2")}
        </p>
        <Switch
          label={t("settings.cameraSwitchLabel")}
          checked={s.camera_enabled ?? true}
          onChange={(checked) => setS({ ...s, camera_enabled: checked })}
        />
      </SettingsCard>

      <SettingsCard title={t("settings.dataLocationTitle")}>
        <DataRootCard />
      </SettingsCard>

      <SettingsCard title={t("settings.extraArgsTitle")}>
        <p className="m-0 mb-2 text-paragraph-xs text-text-soft-400">
          {t("settings.extraArgsHelp1")}<strong>{t("settings.extraArgsLastWord")}</strong>{t("settings.extraArgsHelp2")}
          <br />
          {t("settings.extraArgsHelp3")}
        </p>
        <Textarea
          rows={3}
          className="mono"
          value={s.extra_args ?? ""}
          onChange={(e) => setS({ ...s, extra_args: e.target.value })}
          placeholder={"--disable-background-timer-throttling\n--window-size=1280,800"}
        />
      </SettingsCard>

      <SettingsCard title={t("settings.apiTitle")}>
        <p className="m-0 mb-2 text-paragraph-xs text-text-soft-400">
          {t("settings.apiHelp1")}<strong>127.0.0.1</strong>{t("settings.apiHelp2")}{" "}
          <a
            href="#"
            className="text-primary-base hover:underline"
            onClick={(e) => {
              e.preventDefault();
              openUrl(withUtm("https://docs.proxyshard.com/eng/shardx-launcher-api/binding-and-lifecycle?fallback=true")).catch(() => {});
            }}
          >
            {t("settings.apiRefLink")}
          </a>
        </p>
        <div className="flex flex-col gap-3">
          <Switch
            label={t("settings.apiEnableLabel")}
            checked={s.api_enabled ?? true}
            onChange={(checked) => setS({ ...s, api_enabled: checked })}
          />
          <Input
            label={t("settings.apiPortLabel")}
            inputSize="small"
            type="number"
            value={s.api_port ?? 40325}
            onChange={(e) => setS({ ...s, api_port: Number(e.target.value) || 40325 })}
          />
          {api && (
            <>
              <label className="flex flex-col gap-1.5">
                <span className="text-label-xs text-text-sub-600">{t("settings.apiBaseUrlLabel")}</span>
                <CopyField value={api.base_url} />
              </label>
              <label className="flex flex-col gap-1.5">
                <span className="text-label-xs text-text-sub-600">{t("settings.apiTokenLabel")}</span>
                <CopyField value={api.token} secret />
              </label>
              <div className="mt-1 flex items-center gap-2.5">
                <Button variant="neutral" mode="stroke" size="small" onClick={regenToken}>
                  {t("settings.apiRegenerateBtn")}
                </Button>
                <span className="text-paragraph-xs text-text-soft-400">{t("settings.apiRegenerateHint")}</span>
              </div>
              <p className="m-0 text-paragraph-xs text-text-soft-400">
                {t("settings.apiAuthHeaderHint")}<code>Authorization: Bearer &lt;token&gt;</code>.
              </p>
            </>
          )}
        </div>
      </SettingsCard>

      <SettingsCard title={t("settings.mcpTitle")}>
        <p className="m-0 mb-2 text-paragraph-xs text-text-soft-400">
          {t("settings.mcpHelp1")}<strong>MCP</strong>{t("settings.mcpHelp2")}
        </p>
        <Button
          variant="neutral"
          mode="stroke"
          size="small"
          leftIcon={<DownloadIcon className="size-4" />}
          onClick={downloadMcp}
          disabled={mcpBusy}
          isLoading={mcpBusy}
        >
          {mcpBusy ? t("settings.mcpDownloading") : t("settings.mcpDownloadBtn")}
        </Button>
      </SettingsCard>

      <div className="mt-3.5">
        <Button
          variant="primary"
          mode="filled"
          size="small"
      //    leftIcon={<ShardMini />}
          onClick={async () => { await save(); refreshApi(); }}
        >
          {t("settings.saveBtn")}
        </Button>
      </div>
    </section>
  );
}
