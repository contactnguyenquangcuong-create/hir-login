import { useState } from "react";
import { Button } from "@proxyshard/shardx-ui-kit";
import { toast } from "../../shared/model/toast";
import { confirmModal } from "../../shared/model/confirm";
import { syncLogTail, syncPushAll } from "../../entities/settings";
import { Section, Row } from "./ui";

/** Two things for when a machine arrives logged out: send this machine's logins up (from the
 *  machine that has them), and read what the sync did here — the lines say how many cookies
 *  were made portable, how many were restored, and when a bundle carried no login at all. */
export function SyncLogPanel() {
  const [log, setLog] = useState<string | null>(null);
  const [busy, setBusy] = useState<"log" | "push" | null>(null);

  const showLog = async () => {
    setBusy("log");
    try { setLog(await syncLogTail(150)); }
    catch (e) { toast.err(String(e)); }
    finally { setBusy(null); }
  };

  const pushAll = async () => {
    const ok = await confirmModal({
      title: "Đẩy lại đăng nhập",
      message:
        "Gửi toàn bộ profile trên máy NÀY (kèm cookie đăng nhập) lên server, thay bản đang có ở đó. " +
        "Chỉ làm trên máy đang giữ bản đã đăng nhập đúng — máy khác sẽ nhận bản này khi mở profile.",
      buttons: [
        { label: "Huỷ", value: false },
        { label: "Đẩy lên server", value: true, primary: true },
      ],
    });
    if (!ok) return;
    setBusy("push");
    try {
      const r = await syncPushAll();
      toast.ok(`Đã đẩy ${r.sent} profile lên server${r.skipped ? ` · bỏ qua ${r.skipped} (đang mở hoặc bị máy khác giữ)` : ""}`);
      if (log !== null) setLog(await syncLogTail(150));
    } catch (e) { toast.err(String(e)); }
    finally { setBusy(null); }
  };

  return (
    <Section
      title="Đăng nhập & nhật ký đồng bộ"
      desc="Máy mới vào mà phải đăng nhập lại? Xem nhật ký để biết gói profile có kèm cookie không; máy đang giữ bản đã đăng nhập thì bấm “Đẩy lại đăng nhập”."
    >
      <Row
        label="Đẩy lại đăng nhập lên server"
        hint="Dùng trên máy đang có các profile đã đăng nhập sẵn, sau khi cập nhật bản mới: bản cũ trên server có thể không mang theo cookie."
      >
        <div className="sm:flex sm:justify-end">
          <Button variant="neutral" mode="stroke" size="xsmall" isLoading={busy === "push"} disabled={busy !== null} onClick={() => void pushAll()}>
            Đẩy lại đăng nhập
          </Button>
        </div>
      </Row>
      <Row label="Nhật ký đồng bộ" hint="Những dòng gần nhất — gửi cho người hỗ trợ khi cần.">
        <div className="sm:flex sm:justify-end">
          <Button variant="neutral" mode="stroke" size="xsmall" isLoading={busy === "log"} disabled={busy !== null} onClick={() => void showLog()}>
            {log === null ? "Xem nhật ký" : "Tải lại"}
          </Button>
        </div>
      </Row>
      {log !== null && (
        <div className="flex flex-col gap-2 px-5 py-3.5">
          <pre className="m-0 max-h-72 overflow-auto whitespace-pre-wrap break-all rounded-8 bg-bg-weak-50 p-3 font-mono text-[11px] text-text-sub-600">
            {log.trim() === "" ? "Chưa có gì được ghi — mở một profile đang đồng bộ rồi bấm “Tải lại”." : log}
          </pre>
          <div className="flex justify-end">
            <Button
              variant="neutral" mode="ghost" size="xsmall"
              onClick={() => navigator.clipboard.writeText(log).then(() => toast.ok("Đã sao chép nhật ký"), () => toast.err("Không sao chép được"))}
            >
              Sao chép
            </Button>
          </div>
        </div>
      )}
    </Section>
  );
}
