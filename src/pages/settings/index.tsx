import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import { Button, Input, Select, Switch, Textarea } from "@proxyshard/shardx-ui-kit";
import { DownloadIcon } from "../../shared/icons";
import { Topbar } from "../../shared/ui/Topbar";
import { CopyField } from "../../shared/ui/CopyField";
import { MembersPanel } from "./MembersPanel";
import { toast } from "../../shared/model/toast";
import { withUtm } from "../../shared/lib/utils";
import type { Settings, ApiInfo, RemoteProfileStatus } from "../../entities/settings";
import { HELPER_KINDS } from "../../entities/settings";
import { settingsGet, settingsSave, settingsLoadError, apiInfo, apiRegenerateToken, mcpDownload, teamSyncList, teamSyncPull, teamServerStart, teamServerStop, teamServerStatus, teamInviteGenerate, teamInviteGenerateWithAuth, teamInviteJoin, tailscaleStatus, autostartGet, autostartSet } from "../../entities/settings";
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
        <p className="m-0 mb-3 text-paragraph-xs text-text-soft-400">{t("settings.syncHelp1")}</p>
        <SyncSection
          sync={s.sync}
          onSyncChange={(patch) => setS({ ...s, sync: { ...s.sync!, ...patch } as typeof s.sync })}
          onDisconnect={() => setS({ ...s, sync: { ...s.sync!, enabled: false, server_url: null, token: null } })}
          syncTesting={syncTesting}
          syncRows={syncRows}
          testSync={testSync}
        />
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

function SyncSection({
  sync,
  onSyncChange,
  onDisconnect,
  syncTesting,
  syncRows,
  testSync,
}: {
  sync: Settings["sync"];
  onSyncChange: (patch: Record<string, unknown>) => void;
  onDisconnect: () => void;
  syncTesting: boolean;
  syncRows: RemoteProfileStatus[] | null;
  testSync: () => void;
}) {
  const t = useT();
  const [serverRunning, setServerRunning] = useState(false);
  const [serverPort, setServerPort] = useState(8787);
  const [serverIp, setServerIp] = useState<string | null>(null);
  const [serverBusy, setServerBusy] = useState(false);
  const [inviteCode, setInviteCode] = useState<string | null>(null);
  const [authKey, setAuthKey] = useState("");
  const [joinCode, setJoinCode] = useState("");
  const [joinBusy, setJoinBusy] = useState(false);
  const [tsInstalled, setTsInstalled] = useState<boolean | null>(null);
  const [advancedOpen, setAdvancedOpen] = useState(false);
  const [pulling, setPulling] = useState(false);
  const [autoStart, setAutoStart] = useState(false);

  const isConnected = !!(sync?.enabled && sync?.server_url && sync?.token);
  const connectedUrl = sync?.server_url ?? "";

  const refreshServer = async () => {
    try {
      const st = await teamServerStatus();
      setServerRunning(st.running);
      if (st.running) { setServerPort(st.port ?? 8787); setServerIp(st.tailscale_ip ?? null); }
    } catch {}
  };
  const refreshTs = () => tailscaleStatus().then((s) => setTsInstalled(s.installed)).catch(() => setTsInstalled(false));
  useEffect(() => { refreshServer(); refreshTs(); autostartGet().then(setAutoStart).catch(() => {}); }, []);

  // After a restart the server resumes on its own, but the invite code lived only in
  // this page's state — rebuild it (it is derived from URL + token, so it is the same code).
  useEffect(() => {
    const token = (sync?.token ?? "").trim();
    if (!serverRunning || inviteCode || token.length < 8) return;
    teamInviteGenerate(`http://${serverIp ?? "127.0.0.1"}:${serverPort}`, token)
      .then(setInviteCode)
      .catch(() => {});
  }, [serverRunning, serverIp, serverPort, sync?.token, inviteCode]);

  const toggleServer = async () => {
    setServerBusy(true);
    try {
      if (serverRunning) {
        await teamServerStop();
        setServerRunning(false);
        setInviteCode(null);
      } else {
        const token = (sync?.token ?? "").trim();
        if (token.length < 8) { toast.err("Nhập Token chung (≥8 ký tự) trước khi bật server."); return; }
        const actualPort = await teamServerStart(serverPort, token);
        setServerPort(actualPort);
        setServerRunning(true);
        const st = await teamServerStatus();
        setServerIp(st.tailscale_ip ?? null);
        const hostIp = st.tailscale_ip ?? "127.0.0.1";
        const url = `http://${hostIp}:${actualPort}`;
        const code = await teamInviteGenerate(url, token);
        setInviteCode(code);
        // host itself is also synced
        onSyncChange({ enabled: true, server_url: url, token, device_name: sync?.device_name ?? null, slim_local: sync?.slim_local ?? true });
      }
    } catch (e) { toast.err(String(e)); }
    finally { setServerBusy(false); }
  };

  const genCode = async () => {
    const token = (sync?.token ?? "").trim();
    if (!token) { toast.err("Chưa có token."); return; }
    const hostIp = serverIp ?? "127.0.0.1";
    const url = `http://${hostIp}:${serverPort}`;
    try {
      const ak = authKey.trim() || undefined;
      const code = ak ? await teamInviteGenerateWithAuth(url, token, ak) : await teamInviteGenerate(url, token);
      setInviteCode(code);
    } catch (e) { toast.err(String(e)); }
  };

  const join = async () => {
    if (!joinCode.trim()) return;
    setJoinBusy(true);
    try {
      const res = await teamInviteJoin(joinCode.trim());
      // team_invite_join already saved settings on disk — reflect in UI
      onSyncChange({ enabled: true, server_url: res.url, token: res.token as string });
      toast.ok(`Đã kết nối tới ${res.url}`);
      setJoinCode("");
      refreshTs();
    } catch (e) { toast.err(String(e)); }
    finally { setJoinBusy(false); }
  };

  return (
    <div className="flex flex-col gap-3">
      {/* Connection badge */}
      {isConnected && !serverRunning && (
        <div className="flex items-center justify-between rounded-lg bg-success-alpha-10 px-3 py-2 ring-1 ring-inset ring-success-alpha-16">
          <span className="text-paragraph-xs text-success-base">Đã kết nối tới <span className="mono font-medium">{connectedUrl}</span></span>
          <div className="flex items-center gap-2">
            <Button variant="neutral" mode="stroke" size="small" isLoading={pulling} onClick={async () => {
              setPulling(true);
              try {
                const n = await teamSyncPull();
                if (n === 0) toast.ok("Đã đồng bộ — không có profile mới");
                else { toast.ok(`Đã kéo ${n} profile mới — tắt mở lại danh sách sẽ thấy`); window.dispatchEvent(new CustomEvent("store-changed")); }
              } catch (e) { toast.err(String(e)); }
              finally { setPulling(false); }
            }}>Đồng bộ ngay</Button>
            <Button variant="neutral" mode="stroke" size="small" onClick={onDisconnect}>Ngắt</Button>
          </div>
        </div>
      )}
      {serverRunning && (
        <div className="flex items-center gap-2 rounded-lg bg-primary-alpha-10 px-3 py-2 ring-1 ring-inset ring-primary-alpha-16">
          <span className="text-paragraph-xs text-primary-base">Đang làm máy chủ :{serverPort}</span>
          {serverIp && <span className="mono text-paragraph-xs text-text-sub-600">· {serverIp}</span>}
          <span className="ml-auto text-paragraph-xs text-success-base">Đã kết nối</span>
        </div>
      )}

      {/* Join by code — hidden when already hosting (host already has a code) */}
      {!serverRunning && (
        <div className="flex flex-col gap-2 rounded-lg bg-bg-weak-50 p-3">
          <span className="text-label-xs text-text-sub-600">Tham gia team bằng mã</span>
          {isConnected ? (
            <p className="m-0 text-paragraph-xs text-text-soft-400">Đã tham gia. Mã mới sẽ ghi đè kết nối hiện tại.</p>
          ) : (
            <p className="m-0 text-paragraph-xs text-text-soft-400">Dán mã <span className="mono">HIR-XXXX-...</span> mà admin gửi để tự kết nối.</p>
          )}
          <div className="flex gap-2">
            <Input inputSize="small" value={joinCode} onChange={(e) => setJoinCode(e.target.value)} placeholder="HIR-XXXX-XXXX-..." className="flex-1" />
            <Button variant="primary" mode="filled" size="small" onClick={join} isLoading={joinBusy}>Kết nối</Button>
          </div>
          {tsInstalled === false && !isConnected && (
            <div className="flex flex-wrap items-center gap-2 text-paragraph-xs">
              <span className="text-warning-base">Chưa cài HirLogin Server</span>
              <Button variant="neutral" mode="stroke" size="small" onClick={() => openUrl("https://tailscale.com/download").catch(() => {})}>Cài HirLogin Server</Button>
              <span className="text-text-soft-400">(cài xong dán mã ở trên là tự kết nối)</span>
            </div>
          )}
        </div>
      )}

      {/* Host card */}
      <div className="flex flex-col gap-2 rounded-lg bg-bg-weak-50 p-3">
        <span className="text-label-xs text-text-sub-600">Làm máy chủ (PC online 24/24)</span>
        <p className="m-0 text-paragraph-xs text-text-soft-400">Bật để máy này làm HirLogin Server cho cả team, sinh mã cho nhân sự.</p>
        <div className="flex items-center gap-2">
          <Button variant={serverRunning ? "neutral" : "primary"} mode={serverRunning ? "stroke" : "filled"} size="small" onClick={toggleServer} isLoading={serverBusy}>
            {serverRunning ? "Tắt server" : "Bật server"}
          </Button>
          <span className={`text-paragraph-xs ${serverRunning ? "text-success-base" : "text-text-soft-400"}`}>{serverRunning ? `Đang chạy :${serverPort}` : "Đang tắt"}</span>
          {serverRunning && serverIp && <span className="mono text-paragraph-xs text-text-sub-600">IP: {serverIp}</span>}
        </div>
        <Switch
          label="Tự mở khi bật máy (server tự chạy lại, thu nhỏ vào khay)"
          checked={autoStart}
          onChange={async (v) => { try { await autostartSet(v); setAutoStart(v); toast.ok(v ? "Sẽ tự mở khi bật máy" : "Đã tắt tự mở"); } catch (e) { toast.err(String(e)); } }}
        />
        {!serverRunning && (
          <div className="flex items-center gap-2">
            <Input inputSize="small" type="number" value={serverPort} onChange={(e) => setServerPort(Number(e.target.value) || 8787)} label="Port" className="w-28" />
            <Input inputSize="small" type="password" value={sync?.token ?? ""} onChange={(e) => onSyncChange({ token: e.target.value })} placeholder="Token chung (≥8 ký tự)" label="Token chung" className="flex-1" />
          </div>
        )}
        {serverRunning && (
          <div className="flex flex-col gap-1.5">
            <Input inputSize="small" value={authKey} onChange={(e) => setAuthKey(e.target.value)} placeholder="Tailscale Auth Key (gộp vào mã, tùy chọn)" label="Auth Key" className="flex-1" />
            <Button variant="neutral" mode="stroke" size="small" onClick={genCode}>Tạo mã team</Button>
            {inviteCode && <CopyField value={inviteCode} />}
            <p className="m-0 text-paragraph-xs text-text-soft-400">Gửi mã này cho nhân sự. Nếu có Auth Key trong mã, họ chỉ cần cài HirLogin Server + dán mã là tự vào mạng.</p>
          </div>
        )}
      </div>

      {serverRunning && <MembersPanel serverUrl={`http://${serverIp ?? "127.0.0.1"}:${serverPort}`} />}

      {/* Advanced — replaces the old standalone enabled/url/token block */}
      <button type="button" onClick={() => setAdvancedOpen((v) => !v)} className="self-start text-paragraph-xs text-text-soft-400 hover:text-text-sub-600">
        {advancedOpen ? "▾ Ẩn cấu hình nâng cao" : "▸ Cấu hình nâng cao"}
      </button>
      {advancedOpen && (
        <div className="flex flex-col gap-3 rounded-lg border border-stroke-soft-200 p-3">
          <p className="m-0 text-paragraph-xs text-text-soft-400">Các ô dưới tự điền khi dùng mã team. Chỉ chỉnh tay nếu bạn tự host sync server riêng.</p>
          <Input
            label={t("settings.syncServerUrlLabel")}
            inputSize="small"
            value={sync?.server_url ?? ""}
            onChange={(e) => onSyncChange({ server_url: e.target.value, enabled: !!(e.target.value.trim() && (sync?.token ?? "").trim()) })}
            placeholder="http://100.x.x.x:8787  hoặc  https://sync.yourdomain.com"
          />
          <Input
            label={t("settings.syncTokenLabel")}
            inputSize="small"
            type="password"
            value={sync?.token ?? ""}
            onChange={(e) => onSyncChange({ token: e.target.value, enabled: !!(e.target.value.trim() && (sync?.server_url ?? "").trim()) })}
          />
          <Switch
            label={t("settings.syncSlimLocalLabel")}
            checked={sync?.slim_local ?? true}
            onChange={(checked) => onSyncChange({ slim_local: checked })}
          />
          <p className="m-0 text-paragraph-xs text-text-soft-400">{t("settings.syncSlimLocalHint")}</p>
          <Input
            label={t("settings.syncDeviceNameLabel")}
            inputSize="small"
            value={sync?.device_name ?? ""}
            onChange={(e) => onSyncChange({ device_name: e.target.value })}
            placeholder={t("settings.syncDeviceNamePlaceholder")}
          />
          <div className="flex items-center gap-2.5">
            <Button variant="neutral" mode="stroke" size="small" onClick={testSync} isLoading={syncTesting}>{t("settings.syncTestBtn")}</Button>
            <span className="text-paragraph-xs text-text-soft-400">{t("settings.syncTestHint")}</span>
          </div>
          {syncRows && (
            <div className="flex flex-col gap-1 rounded-lg bg-bg-weak-50 p-2.5 text-paragraph-xs">
              {syncRows.length === 0 && <span className="text-text-soft-400">{t("settings.syncNoProfilesYet")}</span>}
              {syncRows.map((r) => (
                <div key={r.id} className="flex items-center justify-between gap-2">
                  <span className="mono truncate text-text-sub-600">{r.id}</span>
                  <span className={r.locked ? "text-warning-base" : "text-success-base"}>{r.locked ? t("settings.syncLockedBy", { holder: r.holder ?? "?" }) : t("settings.syncFree")}</span>
                </div>
              ))}
            </div>
          )}
        </div>
      )}
    </div>
  );
}
