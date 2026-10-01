import { useEffect, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { Button, Input, Switch } from "@proxyshard/shardx-ui-kit";
import { toast } from "../../shared/model/toast";
import { useT } from "../../shared/i18n";
import { startTeamRole, useTeam } from "../../shared/model/teamRole";
import type { Settings, RemoteProfileStatus } from "../../entities/settings";
import { teamSyncList, teamSyncPull, teamServerStart, teamServerStop, teamServerStatus, teamServerConflict, teamSyncedElsewhere, firewallStatus, firewallGrant, type FirewallStatus, teamInviteJoin, tailscaleStatus, autostartGet, autostartSet, tailscaleOauthGet, tailscaleOauthSet, tailscaleOauthClear } from "../../entities/settings";
import { confirmModal } from "../../shared/model/confirm";
import { MembersPanel } from "./MembersPanel";
import { TailscaleKeysPanel } from "./TailscaleKeysPanel";
import { Section, Row, Block, Pill, Dot, Segmented } from "./ui";

const ROLE_LABEL = { admin: "Quản trị", manager: "Quản lý nhóm", member: "Thành viên" } as const;

export function TeamTab({
  sync, onSyncChange, onSyncCommit, onDisconnect,
}: {
  sync: Settings["sync"];
  onSyncChange: (patch: Record<string, unknown>) => void;
  // Like onSyncChange, but persists to disk immediately instead of waiting
  // for "Lưu cài đặt". Every backend call reads the token from the saved
  // settings.json, not from this page's in-memory state — so a token handed
  // out by joining or starting a server must land on disk before anything
  // else (a `/me` refresh, an admin action) tries to use it, or that request
  // reads the previous, now-wrong token and gets a bogus 401.
  onSyncCommit: (patch: Record<string, unknown>) => Promise<void>;
  onDisconnect: () => void;
}) {
  const t = useT();
  const role = useTeam((s) => s.role);
  const [mode, setMode] = useState<"join" | "host">("join");
  const [serverRunning, setServerRunning] = useState(false);
  const [fw, setFw] = useState<FirewallStatus | null>(null);
  const [conflict, setConflict] = useState(false);
  const [syncedElsewhere, setSyncedElsewhere] = useState(false);
  const [fwBusy, setFwBusy] = useState(false);
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
  // Re-reads the live Tailscale IP right before minting a code — the Tailscale
  // address can change while this page sits open (reconnect, network switch),
  // and a code baked from a few minutes ago would send the new person to an
  // address that no longer answers.
  const getServerUrl = async () => {
    if (!serverRunning) return sync?.server_url ?? "";
    const st = await teamServerStatus().catch(() => null);
    if (st?.running) { setServerIp(st.tailscale_ip ?? null); return `http://${st.tailscale_ip ?? "127.0.0.1"}:${serverPort}`; }
    return serverUrl;
  };

  const refreshServer = async () => {
    try {
      const st = await teamServerStatus();
      setServerRunning(st.running);
      if (st.running) { setServerPort(st.port ?? 8787); setServerIp(st.tailscale_ip ?? null); setMode("host"); }
    } catch { /* ignore */ }
  };
  const refreshTs = () => tailscaleStatus().then((s) => setTsInstalled(s.installed)).catch(() => setTsInstalled(false));
  const refreshFw = () => firewallStatus().then(setFw).catch(() => setFw(null));
  // One admin prompt adds a rule for the app itself, so every port it uses is open.
  const grantFw = async () => {
    setFwBusy(true);
    try { await firewallGrant(); toast.ok("Đã cấp quyền mạng cho Hir-Login"); }
    catch (e) { toast.err(String(e)); }
    finally { setFwBusy(false); refreshFw(); }
  };
  const refreshConflict = () => teamServerConflict().then(setConflict).catch(() => setConflict(false));
  const refreshSyncedElsewhere = () => teamSyncedElsewhere().then(setSyncedElsewhere).catch(() => setSyncedElsewhere(false));
  useEffect(() => { startTeamRole(); refreshServer(); refreshTs(); refreshFw(); refreshConflict(); refreshSyncedElsewhere(); autostartGet().then(setAutoStart).catch(() => {}); }, []);

  const toggleServer = async () => {
    setServerBusy(true);
    try {
      if (serverRunning) {
        const synCleared = await teamServerStop();
        setServerRunning(false);
        // The file just changed under us — `sync` here is the Settings page's
        // own loaded-once copy, so nothing else tells it that.
        if (synCleared) onSyncChange({ enabled: false, server_url: null, token: null });
        useTeam.getState().refresh();
      } else {
        const token = (sync?.token ?? "").trim();
        if (token.length < 8) { toast.err("Nhập Token quản trị (từ 8 ký tự) trước khi bật server."); return; }
        const actualPort = await teamServerStart(serverPort, token);
        setServerPort(actualPort);
        setServerRunning(true);
        const st = await teamServerStatus();
        setServerIp(st.tailscale_ip ?? null);
        const url = `http://${st.tailscale_ip ?? "127.0.0.1"}:${actualPort}`;
        await onSyncCommit({ enabled: true, server_url: url, token, device_name: sync?.device_name ?? null, slim_local: sync?.slim_local ?? true });
        useTeam.getState().refresh();
        // Other machines can only reach this one if the firewall lets the app in.
        const f = await firewallStatus().catch(() => null);
        setFw(f);
        if (f?.supported && !f.granted) await grantFw();
      }
      refreshConflict();
      refreshSyncedElsewhere();
    } catch (e) { toast.err(String(e)); }
    finally { setServerBusy(false); }
  };

  const join = async () => {
    if (!joinCode.trim()) return;
    setJoinBusy(true);
    try {
      const res = await teamInviteJoin(joinCode.trim());
      await onSyncCommit({ enabled: true, server_url: res.url, token: res.token as string });
      toast.ok(`Đã kết nối tới ${res.url}`);
      setJoinCode("");
      refreshTs();
      useTeam.getState().refresh();
      refreshConflict();
      refreshSyncedElsewhere();
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
      <Section title="Kết nối">
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
                <Button variant="neutral" mode="stroke" size="small" onClick={async () => { await onDisconnect(); refreshSyncedElsewhere(); refreshConflict(); }}>Ngắt</Button>
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

      {conflict && (
        <Section title="⚠️ Máy này đang dính 2 team cùng lúc" desc="Máy này vừa làm máy chủ cho team của bạn, vừa đang là thành viên của một team khác — rất dễ nhầm hồ sơ giữa hai bên. Nếu không cố ý, hãy tắt một trong hai: tắt máy chủ ở dưới, hoặc vào mục Nâng cao xoá kết nối tới team kia.">
          <div />
        </Section>
      )}

      <div><Segmented value={mode} onChange={setMode} options={[{ value: "join", label: "Tham gia team" }, { value: "host", label: "Làm máy chủ" }]} /></div>

      {mode === "join" && (
        <Section title="Tham gia team bằng mã" desc="Dán mã quản trị gửi cho bạn (bắt đầu bằng HIR-). Mã mới sẽ thay kết nối hiện tại.">
          <Block>
            {serverRunning && (
              <div className="rounded-lg bg-warning-alpha-10 p-3 text-paragraph-xs text-warning-base">
                Máy này đang làm máy chủ cho team của bạn — tắt máy chủ ở mục "Làm máy chủ" trước khi tham gia team khác.
              </div>
            )}
            <div className="flex gap-2">
              <div className="flex-1"><Input inputSize="small" value={joinCode} onChange={(e) => setJoinCode(e.target.value)} placeholder="HIR-XXXX-XXXX-..." disabled={serverRunning} /></div>
              <Button variant="primary" mode="filled" size="small" onClick={join} isLoading={joinBusy} disabled={!joinCode.trim() || serverRunning}>Kết nối</Button>
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
          {!serverRunning && syncedElsewhere && (
            <Block>
              <div className="rounded-lg bg-warning-alpha-10 p-3 text-paragraph-xs text-warning-base">
                Máy này đang là thành viên của một team khác — vào mục Nâng cao bên dưới, bấm Ngắt để rời team đó trước khi làm máy chủ.
              </div>
            </Block>
          )}
          <Row label="Máy chủ" hint={serverRunning ? `Đang chạy ở cổng ${serverPort}` : "Đang tắt"}>
            <div className="sm:text-right">
              <Button variant={serverRunning ? "neutral" : "primary"} mode={serverRunning ? "stroke" : "filled"} size="small" onClick={toggleServer} isLoading={serverBusy} disabled={!serverRunning && syncedElsewhere}>
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

      {fw?.supported && (
        <Section title="Quyền mạng (tường lửa Windows)" desc="Cho phép Hir-Login và trình duyệt của nó đi qua tường lửa, mọi cổng — hết các hộp hỏi quyền và lỗi không kết nối được. Chỉ cần làm một lần trên mỗi máy.">
          <Row label="Trạng thái" hint={fw.granted ? "Đã cho phép." : "Chưa cho phép. Bấm nút, Windows sẽ hỏi quyền quản trị một lần."}>
            <div className="sm:flex sm:justify-end">
              {fw.granted
                ? <Pill tone="success"><Dot on />Đã cấp</Pill>
                : <Button variant="primary" mode="stroke" size="small" onClick={grantFw} isLoading={fwBusy}>Cấp quyền mạng</Button>}
            </div>
          </Row>
        </Section>
      )}

      {/* Nhân sự/Auth Key belong to whichever team this tab is actually showing —
          hiding them when the tab doesn't match keeps the page from implying you
          can manage a team's people while a banner just above says you can't
          touch that team from here right now (hosting while on "Tham gia", or
          vice versa). */}
      {mode === (serverRunning ? "host" : "join") && (
        <>
          <MembersPanel getServerUrl={getServerUrl} oauthReady={oauthReady} onOpenOauthSetup={() => setOauthOpen(true)} />
          {oauthReady && <TailscaleKeysPanel />}
        </>
      )}

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
