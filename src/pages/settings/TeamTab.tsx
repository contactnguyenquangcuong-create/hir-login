import { useEffect, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { Button, Input, Switch } from "@proxyshard/shardx-ui-kit";
import { toast } from "../../shared/model/toast";
import { useT } from "../../shared/i18n";
import { startTeamRole, useTeam } from "../../shared/model/teamRole";
import type { Settings, RemoteProfileStatus } from "../../entities/settings";
import { teamSyncList, teamSyncPull, teamServerStart, teamServerStop, teamServerStatus, teamInviteJoin, tailscaleStatus, autostartGet, autostartSet, tailscaleOauthGet, tailscaleOauthSet, tailscaleOauthClear } from "../../entities/settings";
import { confirmModal } from "../../shared/model/confirm";
import { MembersPanel } from "./MembersPanel";
import { TailscaleKeysPanel } from "./TailscaleKeysPanel";
import { Section, Row, Block, Pill, Dot, Segmented } from "./ui";

const ROLE_LABEL = { admin: "Quản trị", manager: "Quản lý nhóm", member: "Thành viên" } as const;

export function TeamTab({
  sync, onSyncChange, onDisconnect,
}: {
  sync: Settings["sync"];
  onSyncChange: (patch: Record<string, unknown>) => void;
  onDisconnect: () => void;
}) {
  const t = useT();
  const role = useTeam((s) => s.role);
  const [mode, setMode] = useState<"join" | "host">("join");
  const [serverRunning, setServerRunning] = useState(false);
  const [serverPort, setServerPort] = useState(8787);
  const [serverIp, setServerIp] = useState<string | null>(null);
  const [serverBusy, setServerBusy] = useState(false);
  const [joinCode, setJoinCode] = useState("");
  const [joinBusy, setJoinBusy] = useState(false);
  const [tsInstalled, setTsInstalled] = useState<boolean | null>(null);
  const [pulling, setPulling] = useState(false);
  const [autoStart, setAutoStart] = useState(false);
  const [advancedOpen, setAdvancedOpen] = useState(false);
  const [syncTesting, setSyncTesting] = useState(false);
  const [syncRows, setSyncRows] = useState<RemoteProfileStatus[] | null>(null);
  const [oauth, setOauth] = useState({ clientId: "", hasSecret: false, tag: "" });
  const [oauthEdit, setOauthEdit] = useState({ clientId: "", clientSecret: "", tag: "tag:hirlogin" });
  const [oauthOpen, setOauthOpen] = useState(false);
  const [oauthSaving, setOauthSaving] = useState(false);
  useEffect(() => {
    tailscaleOauthGet().then((o) => {
      setOauth({ clientId: o.client_id, hasSecret: o.has_secret, tag: o.tag });
      setOauthEdit((e) => ({ ...e, clientId: o.client_id, tag: o.tag || "tag:hirlogin" }));
    }).catch(() => {});
  }, []);
  const oauthReady = oauth.hasSecret && !!oauth.clientId;
  const saveOauth = async () => {
    if (!oauthEdit.clientId.trim() || !oauthEdit.clientSecret.trim()) { toast.err("Cần đủ Client ID và Client Secret."); return; }
    setOauthSaving(true);
    try {
      await tailscaleOauthSet(oauthEdit.clientId.trim(), oauthEdit.clientSecret.trim(), oauthEdit.tag.trim() || "tag:hirlogin");
      setOauth({ clientId: oauthEdit.clientId.trim(), hasSecret: true, tag: oauthEdit.tag.trim() || "tag:hirlogin" });
      setOauthEdit((e) => ({ ...e, clientSecret: "" }));
      setOauthOpen(false);
      toast.ok("Đã lưu OAuth Client. Từ giờ tạo Auth Key ngay trong app.");
    } catch (e) { toast.err(String(e)); }
    finally { setOauthSaving(false); }
  };
  const clearOauth = async () => {
    const ok = await confirmModal({
      title: "Xoá cấu hình OAuth Client?",
      message: "App sẽ ngừng tự tạo Auth Key — quay lại phải dán tay như trước. OAuth Client trên Tailscale không bị ảnh hưởng, chỉ xoá khỏi máy này.",
      danger: true,
    });
    if (ok !== true) return;
    try {
      await tailscaleOauthClear();
      setOauth({ clientId: "", hasSecret: false, tag: "" });
      setOauthEdit({ clientId: "", clientSecret: "", tag: "tag:hirlogin" });
      setOauthOpen(false);
      toast.ok("Đã xoá cấu hình OAuth Client.");
    } catch (e) { toast.err(String(e)); }
  };
  const isConnected = !!(sync?.enabled && sync?.server_url && sync?.token);
  const serverUrl = serverRunning ? `http://${serverIp ?? "127.0.0.1"}:${serverPort}` : (sync?.server_url ?? "");

  const refreshServer = async () => {
    try {
      const st = await teamServerStatus();
      setServerRunning(st.running);
      if (st.running) { setServerPort(st.port ?? 8787); setServerIp(st.tailscale_ip ?? null); setMode("host"); }
    } catch { /* ignore */ }
  };
  const refreshTs = () => tailscaleStatus().then((s) => setTsInstalled(s.installed)).catch(() => setTsInstalled(false));
  useEffect(() => { startTeamRole(); refreshServer(); refreshTs(); autostartGet().then(setAutoStart).catch(() => {}); }, []);

  const toggleServer = async () => {
    setServerBusy(true);
    try {
      if (serverRunning) {
        await teamServerStop();
        setServerRunning(false);
      } else {
        const token = (sync?.token ?? "").trim();
        if (token.length < 8) { toast.err("Nhập Token quản trị (từ 8 ký tự) trước khi bật server."); return; }
        const actualPort = await teamServerStart(serverPort, token);
        setServerPort(actualPort);
        setServerRunning(true);
        const st = await teamServerStatus();
        setServerIp(st.tailscale_ip ?? null);
        const url = `http://${st.tailscale_ip ?? "127.0.0.1"}:${actualPort}`;
        onSyncChange({ enabled: true, server_url: url, token, device_name: sync?.device_name ?? null, slim_local: sync?.slim_local ?? true });
        useTeam.getState().refresh();
      }
    } catch (e) { toast.err(String(e)); }
    finally { setServerBusy(false); }
  };

  const join = async () => {
    if (!joinCode.trim()) return;
    setJoinBusy(true);
    try {
      const res = await teamInviteJoin(joinCode.trim());
      onSyncChange({ enabled: true, server_url: res.url, token: res.token as string });
      toast.ok(`Đã kết nối tới ${res.url}`);
      setJoinCode("");
      refreshTs();
      useTeam.getState().refresh();
    } catch (e) { toast.err(String(e)); }
    finally { setJoinBusy(false); }
  };

  const pull = async () => {
    setPulling(true);
    try {
      const n = await teamSyncPull();
      if (n === 0) toast.ok("Đã đồng bộ, không có profile mới");
      else { toast.ok(`Đã kéo ${n} profile mới`); window.dispatchEvent(new CustomEvent("store-changed")); }
    } catch (e) { toast.err(String(e)); }
    finally { setPulling(false); }
  };

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

  return (
    <div className="flex flex-col gap-4">
      {/* Where this machine stands */}
      <Section title="Kết nối" desc={t("settings.syncHelp1")}>
        <Row
          label={serverRunning ? "Máy này đang làm máy chủ" : isConnected ? "Đã kết nối tới team" : "Chưa kết nối"}
          hint={
            serverRunning ? <span className="mono">{serverIp ?? "127.0.0.1"} · cổng {serverPort}</span>
            : isConnected ? <span className="mono">{sync?.server_url}</span>
            : "Tham gia bằng mã của quản trị, hoặc bật máy này làm máy chủ."
          }
        >
          <div className="flex flex-wrap items-center gap-2 sm:justify-end">
            <Pill tone={serverRunning || isConnected ? "success" : "neutral"}><Dot on={serverRunning || isConnected} />{serverRunning || isConnected ? "Đang hoạt động" : "Đang tắt"}</Pill>
            {isConnected && !serverRunning && (
              <>
                <Button variant="neutral" mode="stroke" size="small" isLoading={pulling} onClick={pull}>Đồng bộ ngay</Button>
                <Button variant="neutral" mode="stroke" size="small" onClick={onDisconnect}>Ngắt</Button>
              </>
            )}
          </div>
        </Row>
        {isConnected && role && (
          <Row label="Vai trò của bạn" hint={role === "member" ? "Chỉ dùng profile trong thư mục được chia sẻ." : role === "manager" ? "Toàn quyền trong thư mục được giao hoặc tự tạo." : "Toàn quyền."}>
            <div className="sm:text-right"><Pill tone={role === "admin" ? "primary" : "neutral"}>{ROLE_LABEL[role]}</Pill></div>
          </Row>
        )}
      </Section>

      <div><Segmented value={mode} onChange={setMode} options={[{ value: "join", label: "Tham gia team" }, { value: "host", label: "Làm máy chủ" }]} /></div>

      {mode === "join" && (
        <Section title="Tham gia team bằng mã" desc="Dán mã quản trị gửi cho bạn (bắt đầu bằng HIR-). Mã mới sẽ thay kết nối hiện tại.">
          <Block>
            <div className="flex gap-2">
              <div className="flex-1"><Input inputSize="small" value={joinCode} onChange={(e) => setJoinCode(e.target.value)} placeholder="HIR-XXXX-XXXX-..." /></div>
              <Button variant="primary" mode="filled" size="small" onClick={join} isLoading={joinBusy} disabled={!joinCode.trim()}>Kết nối</Button>
            </div>
            {tsInstalled === false && !isConnected && (
              <div className="flex flex-wrap items-center gap-2 rounded-lg bg-warning-alpha-10 p-3 text-paragraph-xs">
                <span className="text-warning-base">Máy này chưa cài HirLogin Server (Tailscale).</span>
                <Button variant="neutral" mode="stroke" size="xsmall" onClick={() => openUrl("https://tailscale.com/download").catch(() => {})}>Tải và cài</Button>
                <span className="text-text-soft-400">Cài xong dán mã là tự vào mạng.</span>
              </div>
            )}
          </Block>
        </Section>
      )}

      {mode === "host" && (
        <Section title="Làm máy chủ" desc="Bật ở máy chạy 24/24. Nhân sự kết nối vào máy này để dùng chung profile.">
          <Row label="Máy chủ" hint={serverRunning ? `Đang chạy ở cổng ${serverPort}` : "Đang tắt"}>
            <div className="sm:text-right">
              <Button variant={serverRunning ? "neutral" : "primary"} mode={serverRunning ? "stroke" : "filled"} size="small" onClick={toggleServer} isLoading={serverBusy}>
                {serverRunning ? "Tắt máy chủ" : "Bật máy chủ"}
              </Button>
            </div>
          </Row>
          {!serverRunning && (
            <>
              <Row label="Cổng"><Input inputSize="small" type="number" value={serverPort} onChange={(e) => setServerPort(Number(e.target.value) || 8787)} /></Row>
              <Row label="Token quản trị" hint="Chìa khoá của riêng bạn (từ 8 ký tự). Nhân sự sẽ có mã riêng, không dùng token này.">
                <Input inputSize="small" type="password" value={sync?.token ?? ""} onChange={(e) => onSyncChange({ token: e.target.value })} placeholder="Ít nhất 8 ký tự" />
              </Row>
            </>
          )}
          <Row label="Tự mở khi bật máy" hint="Máy chủ tự chạy lại và thu nhỏ vào khay hệ thống.">
            <div className="sm:flex sm:justify-end">
              <Switch checked={autoStart} onChange={async (v) => { try { await autostartSet(v); setAutoStart(v); toast.ok(v ? "Sẽ tự mở khi bật máy" : "Đã tắt tự mở"); } catch (e) { toast.err(String(e)); } }} />
            </div>
          </Row>
          {serverRunning && (
            <Block>
              <div className="flex flex-col gap-0.5">
                <span className="text-label-sm text-text-strong-950">Tự động tạo Auth Key cho nhân sự</span>
                <span className="text-paragraph-xs text-text-soft-400">
                  Thiết lập một lần. Sau đó mỗi mã ở mục <b>Nhân sự</b> bên dưới tự kèm quyền vào mạng Tailscale — không cần dán tay.
                </span>
              </div>
              {oauthReady ? (
                <div className="flex flex-col gap-1.5">
                  <p className="m-0 text-paragraph-xs text-text-soft-400">
                    Đã bật, thẻ <code className="mono">{oauth.tag}</code>. Mỗi mã nhân sự tự kèm một Auth Key riêng (hạn 90 ngày).
                  </p>
                  <div className="flex gap-3">
                    <button type="button" className="self-start border-0 bg-transparent p-0 text-paragraph-xs text-text-soft-400 underline hover:text-text-sub-600" onClick={() => setOauthOpen((v) => !v)}>
                      Đổi Client ID/Secret
                    </button>
                    <button type="button" className="self-start border-0 bg-transparent p-0 text-paragraph-xs text-text-soft-400 underline hover:text-error-base" onClick={clearOauth}>
                      Xoá cấu hình OAuth
                    </button>
                  </div>
                </div>
              ) : (
                <div>
                  <Button variant="primary" mode="stroke" size="small" onClick={() => setOauthOpen((v) => !v)}>Thiết lập</Button>
                </div>
              )}
              {oauthOpen && (
                <div className="flex flex-col gap-2 rounded-lg bg-bg-weak-50 p-3 ring-1 ring-inset ring-stroke-soft-200">
                  <span className="text-paragraph-xs text-text-sub-600">
                    Tạo 1 lần ở <a href="#" className="text-primary-base hover:underline" onClick={(e) => { e.preventDefault(); openUrl("https://login.tailscale.com/admin/settings/oauth").catch(() => {}); }}>console.tailscale.com → Access controls → OAuth clients</a>: thêm 1 thẻ tên tuỳ ý (ví dụ <code className="mono">tag:hirlogin</code> hoặc <code className="mono">tag:halo</code> — đặt gì cũng được) rồi tạo OAuth Client với quyền <code className="mono">auth_keys</code>, gán đúng thẻ đó. Dán Client ID/Secret vào đây.
                  </span>
                  <Input inputSize="small" value={oauthEdit.clientId} onChange={(e) => setOauthEdit((s) => ({ ...s, clientId: e.target.value }))} label="Client ID" placeholder="k..." />
                  <Input inputSize="small" type="password" value={oauthEdit.clientSecret} onChange={(e) => setOauthEdit((s) => ({ ...s, clientSecret: e.target.value }))} label="Client Secret" placeholder={oauth.hasSecret ? "Đã lưu — dán lại nếu muốn đổi" : "tskey-client-..."} />
                  <Input inputSize="small" value={oauthEdit.tag} onChange={(e) => setOauthEdit((s) => ({ ...s, tag: e.target.value }))} label="Thẻ (tag) đã gán cho OAuth Client — gõ đúng tên bạn đã đặt" placeholder="vd: tag:hirlogin" />
                  <div><Button variant="primary" mode="filled" size="small" onClick={saveOauth} isLoading={oauthSaving}>Lưu</Button></div>
                </div>
              )}
            </Block>
          )}
        </Section>
      )}

      <MembersPanel serverUrl={serverUrl} oauthReady={oauthReady} onOpenOauthSetup={() => setOauthOpen(true)} />

      {oauthReady && <TailscaleKeysPanel />}

      <button type="button" onClick={() => setAdvancedOpen((v) => !v)} className="self-start border-0 bg-transparent p-0 text-label-xs text-text-sub-600 hover:text-text-strong-950">
        {advancedOpen ? "▾ Ẩn cấu hình nâng cao" : "▸ Cấu hình nâng cao"}
      </button>
      {advancedOpen && (
        <Section title="Nâng cao" desc="Tự điền khi dùng mã. Chỉ chỉnh tay nếu bạn tự host sync server riêng.">
          <Row label={t("settings.syncServerUrlLabel")}>
            <Input inputSize="small" value={sync?.server_url ?? ""} placeholder="http://100.x.x.x:8787"
              onChange={(e) => onSyncChange({ server_url: e.target.value, enabled: !!(e.target.value.trim() && (sync?.token ?? "").trim()) })} />
          </Row>
          <Row label={t("settings.syncTokenLabel")}>
            <Input inputSize="small" type="password" value={sync?.token ?? ""}
              onChange={(e) => onSyncChange({ token: e.target.value, enabled: !!(e.target.value.trim() && (sync?.server_url ?? "").trim()) })} />
          </Row>
          <Row label={t("settings.syncSlimLocalLabel")} hint={t("settings.syncSlimLocalHint")}>
            <div className="sm:flex sm:justify-end"><Switch checked={sync?.slim_local ?? true} onChange={(c) => onSyncChange({ slim_local: c })} /></div>
          </Row>
          <Row label={t("settings.syncDeviceNameLabel")}>
            <Input inputSize="small" value={sync?.device_name ?? ""} placeholder={t("settings.syncDeviceNamePlaceholder")} onChange={(e) => onSyncChange({ device_name: e.target.value })} />
          </Row>
          <Row label={t("settings.syncTestBtn")} hint={t("settings.syncTestHint")}>
            <div className="sm:text-right"><Button variant="neutral" mode="stroke" size="small" onClick={testSync} isLoading={syncTesting}>{t("settings.syncTestBtn")}</Button></div>
          </Row>
          {syncRows && (
            <Block>
              {syncRows.length === 0 && <span className="text-paragraph-xs text-text-soft-400">{t("settings.syncNoProfilesYet")}</span>}
              {syncRows.map((r) => (
                <div key={r.id} className="flex items-center justify-between gap-2 text-paragraph-xs">
                  <span className="mono truncate text-text-sub-600">{r.id}</span>
                  <span className={r.locked ? "text-warning-base" : "text-success-base"}>{r.locked ? t("settings.syncLockedBy", { holder: r.holder ?? "?" }) : t("settings.syncFree")}</span>
                </div>
              ))}
            </Block>
          )}
        </Section>
      )}
    </div>
  );
}
