import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import { Button, Input, Select, Switch, Textarea } from "@proxyshard/shardx-ui-kit";
import { DownloadIcon } from "../../shared/icons";
import { Topbar } from "../../shared/ui/Topbar";
import { CopyField } from "../../shared/ui/CopyField";
import { toast } from "../../shared/model/toast";
import { withUtm } from "../../shared/lib/utils";
import type { Settings, ApiInfo, RemoteProfileStatus } from "../../entities/settings";
import { HELPER_KINDS } from "../../entities/settings";
import { settingsGet, settingsSave, settingsLoadError, apiInfo, apiRegenerateToken, mcpDownload, teamSyncList, teamServerStart, teamServerStop, teamServerStatus, teamInviteGenerate, teamInviteGenerateWithAuth, teamInviteJoin, tailscaleStatus } from "../../entities/settings";
import { DataRootCard } from "../../features/manage-profiles/ui/DataRootCard";
import { useT, useLang, LANG_OPTIONS, type Lang } from "../../shared/i18n";
import type { LicenseInfo } from "../../entities/license";
import { licenseInfo } from "../../entities/license";

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

  const [license, setLicense] = useState<LicenseInfo | null>(null);
  useEffect(() => {
    licenseInfo().then(setLicense).catch(() => {});
  }, []);
  const regenToken = async () => {
    try { setApi(await apiRegenerateToken()); toast.ok(t("settings.tokenRegenerated")); }
    catch (e) { toast.err(String(e)); }
  };

  const [syncTesting, setSyncTesting] = useState(false);
  const [syncRows, setSyncRows] = useState<RemoteProfileStatus[] | null>(null);
  const testSync = async () => {
    setSyncTesting(true);
    setSyncRows(null);
    try {
      const rows = await teamSyncList();
      setSyncRows(rows);
      toast.ok(t("settings.syncTestOk", { n: rows.length }));
    } catch (e) { toast.err(String(e)); }
    finally { setSyncTesting(false); }
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

      {license && (
        <SettingsCard title={t("settings.licenseTitle")}>
          <div className="mb-3 flex flex-col gap-1.5 text-paragraph-xs">
            <div className="flex items-center justify-between gap-2">
              <span className="text-text-soft-400">{t("settings.licenseKeyLabel")}</span>
              <span className="mono text-text-strong-950">{license.key}</span>
            </div>
            <div className="flex items-center justify-between gap-2">
              <span className="text-text-soft-400">{t("settings.licenseNameLabel")}</span>
              <span className="text-text-sub-600">{license.customer_name ?? "—"}</span>
            </div>
            <div className="flex items-center justify-between gap-2">
              <span className="text-text-soft-400">{t("settings.licensePhoneLabel")}</span>
              <span className="text-text-sub-600">{license.customer_phone ?? "—"}</span>
            </div>
            <div className="flex items-center justify-between gap-2">
              <span className="text-text-soft-400">{t("settings.licenseEmailLabel")}</span>
              <span className="text-text-sub-600">{license.customer_email ?? "—"}</span>
            </div>
          </div>
          <label className="flex flex-col gap-1.5">
            <span className="text-label-xs text-text-sub-600">{t("settings.licenseDeviceLabel")}</span>
            <CopyField value={license.device_id} />
          </label>
        </SettingsCard>
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

      <SettingsCard title={t("settings.syncTitle")}>
        <p className="m-0 mb-2 text-paragraph-xs text-text-soft-400">
          {t("settings.syncHelp1")}
        </p>
        {/* --- Invite code join (for team members) --- */}
        <div className="mb-3 flex flex-col gap-2 rounded-lg bg-bg-weak-50 p-3">
          <span className="text-label-xs text-text-sub-600">Tham gia team bằng mã</span>
          <p className="m-0 text-paragraph-xs text-text-soft-400">Dán mã team (HIR-XXXX-...) mà admin gửi để tự kết nối. Nếu chưa cài HirLogin Server, bấm nút cài bên dưới.</p>
          <InviteJoinCard />
        </div>

        {/* --- Host mode: run embedded server --- */}
        <div className="mb-3 flex flex-col gap-2 rounded-lg bg-bg-weak-50 p-3">
          <span className="text-label-xs text-text-sub-600">Làm máy chủ (PC online 24/24)</span>
          <p className="m-0 text-paragraph-xs text-text-soft-400">Bật để máy này làm HirLogin Server cho cả team. Chia sẻ mã team cho nhân sự.</p>
          <TeamServerCard sync={s.sync} onTokenChange={(v) => setS({ ...s, sync: { ...s.sync!, token: v } })} />
        </div>

        <div className="flex flex-col gap-3">
          <Switch
            label={t("settings.syncEnableLabel")}
            checked={s.sync?.enabled ?? false}
            onChange={(checked) => setS({ ...s, sync: { ...s.sync, enabled: checked, server_url: s.sync?.server_url ?? null, token: s.sync?.token ?? null, device_name: s.sync?.device_name ?? null, slim_local: s.sync?.slim_local ?? true } })}
          />
          {(s.sync?.enabled ?? false) && (
            <>
              <Input
                label={t("settings.syncServerUrlLabel")}
                inputSize="small"
                value={s.sync?.server_url ?? ""}
                onChange={(e) => setS({ ...s, sync: { ...s.sync!, server_url: e.target.value } })}
                placeholder="http://100.x.x.x:8787  hoặc  https://sync.yourdomain.com"
              />
              <Input
                label={t("settings.syncTokenLabel")}
                inputSize="small"
                type="password"
                value={s.sync?.token ?? ""}
                onChange={(e) => setS({ ...s, sync: { ...s.sync!, token: e.target.value } })}
              />
              <Switch
                label={t("settings.syncSlimLocalLabel")}
                checked={s.sync?.slim_local ?? true}
                onChange={(checked) => setS({ ...s, sync: { ...s.sync!, slim_local: checked } })}
              />
              <p className="m-0 text-paragraph-xs text-text-soft-400">{t("settings.syncSlimLocalHint")}</p>
              <Input
                label={t("settings.syncDeviceNameLabel")}
                inputSize="small"
                value={s.sync?.device_name ?? ""}
                onChange={(e) => setS({ ...s, sync: { ...s.sync!, device_name: e.target.value } })}
                placeholder={t("settings.syncDeviceNamePlaceholder")}
              />
              <div className="flex items-center gap-2.5">
                <Button variant="neutral" mode="stroke" size="small" onClick={testSync} isLoading={syncTesting}>
                  {t("settings.syncTestBtn")}
                </Button>
                <span className="text-paragraph-xs text-text-soft-400">{t("settings.syncTestHint")}</span>
              </div>
              {syncRows && (
                <div className="flex flex-col gap-1 rounded-lg bg-bg-weak-50 p-2.5 text-paragraph-xs">
                  {syncRows.length === 0 && <span className="text-text-soft-400">{t("settings.syncNoProfilesYet")}</span>}
                  {syncRows.map((r) => (
                    <div key={r.id} className="flex items-center justify-between gap-2">
                      <span className="mono truncate text-text-sub-600">{r.id}</span>
                      <span className={r.locked ? "text-warning-base" : "text-success-base"}>
                        {r.locked ? t("settings.syncLockedBy", { holder: r.holder ?? "?" }) : t("settings.syncFree")}
                      </span>
                    </div>
                  ))}
                </div>
              )}
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

function InviteJoinCard() {
  const [code, setCode] = useState("");
  const [busy, setBusy] = useState(false);
  const [tsInstalled, setTsInstalled] = useState<boolean | null>(null);
  useEffect(() => { tailscaleStatus().then((s) => setTsInstalled(s.installed)).catch(() => setTsInstalled(false)); }, []);
  const join = async () => {
    if (!code.trim()) return;
    setBusy(true);
    try {
      const res = await teamInviteJoin(code.trim());
      toast.ok(`Đã kết nối tới ${res.url}`);
    } catch (e) { toast.err(String(e)); }
    finally { setBusy(false); }
  };
  return (
    <div className="flex flex-col gap-2">
      <div className="flex gap-2">
        <Input inputSize="small" value={code} onChange={(e) => setCode(e.target.value)} placeholder="HIR-XXXX-XXXX-..." className="flex-1" />
        <Button variant="primary" mode="filled" size="small" onClick={join} isLoading={busy}>Kết nối</Button>
      </div>
      {tsInstalled === false && (
        <div className="flex items-center gap-2 text-paragraph-xs">
          <span className="text-warning-base">Chưa cài HirLogin Server</span>
          <Button variant="neutral" mode="stroke" size="small" onClick={() => openUrl("https://tailscale.com/download").catch(() => {})}>Cài HirLogin Server</Button>
          <span className="text-text-soft-400">(cài xong dán mã team ở trên là tự kết nối)</span>
        </div>
      )}
    </div>
  );
}

function TeamServerCard({ sync, onTokenChange }: { sync: Settings["sync"]; onTokenChange: (v: string) => void }) {
  const [running, setRunning] = useState(false);
  const [port, setPort] = useState(8787);
  const [ip, setIp] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [inviteCode, setInviteCode] = useState<string | null>(null);
  const [tailscaleKey, setTailscaleKey] = useState("");

  const refresh = async () => {
    try {
      const st = await teamServerStatus();
      setRunning(st.running);
      if (st.running) { setPort(st.port ?? 8787); setIp(st.tailscale_ip ?? null); }
    } catch {}
  };
  useEffect(() => { refresh(); }, []);

  const toggle = async () => {
    setBusy(true);
    try {
      if (running) {
        await teamServerStop();
        setRunning(false);
        setInviteCode(null);
      } else {
        const token = (sync?.token ?? "").trim();
        if (token.length < 8) { toast.err("Nhập token (≥8 ký tự) trước khi bật server."); return; }
        const actualPort = await teamServerStart(port, token);
        setPort(actualPort);
        setRunning(true);
        const st = await teamServerStatus();
        setIp(st.tailscale_ip ?? null);
        const hostIp = st.tailscale_ip ?? "127.0.0.1";
        const url = `http://${hostIp}:${actualPort}`;
        const code = await teamInviteGenerate(url, token);
        setInviteCode(code);
      }
    } catch (e) { toast.err(String(e)); }
    finally { setBusy(false); }
  };

  const genCode = async () => {
    const token = (sync?.token ?? "").trim();
    if (!token) { toast.err("Chưa có token."); return; }
    const hostIp = ip ?? "127.0.0.1";
    const url = `http://${hostIp}:${port}`;
    try {
      const ak = tailscaleKey.trim() || undefined;
      const code = ak ? await teamInviteGenerateWithAuth(url, token, ak) : await teamInviteGenerate(url, token);
      setInviteCode(code);
    } catch (e) { toast.err(String(e)); }
  };

  return (
    <div className="flex flex-col gap-2">
      <div className="flex items-center gap-2">
        <Button variant={running ? "neutral" : "primary"} mode={running ? "stroke" : "filled"} size="small" onClick={toggle} isLoading={busy}>
          {running ? "Tắt server" : "Bật server"}
        </Button>
        <span className={`text-paragraph-xs ${running ? "text-success-base" : "text-text-soft-400"}`}>{running ? `Đang chạy :${port}` : "Đang tắt"}</span>
        {running && ip && <span className="mono text-paragraph-xs text-text-sub-600">Server IP: {ip}</span>}
      </div>
      {!running && (
        <div className="flex items-center gap-2">
          <Input inputSize="small" type="number" value={port} onChange={(e) => setPort(Number(e.target.value) || 8787)} label="Port" className="w-28" />
          <Input inputSize="small" type="password" value={sync?.token ?? ""} onChange={(e) => onTokenChange(e.target.value)} placeholder="Token chung cho team" label="Token" className="flex-1" />
        </div>
      )}
      {running && (
        <div className="flex flex-col gap-1.5">
          <div className="flex items-center gap-2">
            <Input inputSize="small" value={tailscaleKey} onChange={(e) => setTailscaleKey(e.target.value)} placeholder="Tailscale Auth Key (để gộp vào mã team, tùy chọn)" label="Auth Key" className="flex-1" />
          </div>
          <Button variant="neutral" mode="stroke" size="small" onClick={genCode}>Tạo mã team</Button>
          {inviteCode && <CopyField value={inviteCode} />}
          <p className="m-0 text-paragraph-xs text-text-soft-400">Gửi mã này cho nhân sự. Nếu có Auth Key trong mã, nhân sự chỉ cần cài HirLogin Server + dán mã là tự vào mạng.</p>
        </div>
      )}
    </div>
  );
}
