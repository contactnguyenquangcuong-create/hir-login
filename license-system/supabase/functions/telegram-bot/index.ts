// Hir-Login license-key admin bot — Supabase Edge Function (Deno).
// Telegram pushes each message here via webhook; no VPS, no always-on
// process to babysit — Supabase hosts this function itself.
//
// Deploy once:
//   supabase functions deploy telegram-bot --no-verify-jwt
//   supabase secrets set TELEGRAM_BOT_TOKEN=123:abc ADMIN_IDS=123456789
//   curl "https://api.telegram.org/bot<TOKEN>/setWebhook?url=https://<project-ref>.supabase.co/functions/v1/telegram-bot"
//
// SUPABASE_URL and SUPABASE_SERVICE_ROLE_KEY are injected automatically by
// Supabase for every Edge Function — you do not set those yourself.
//
// Commands (DM the bot, admin only — everyone else is ignored):
//   /taomoi [ghi chú]  Create a key, optionally tagged with a note.
//   /ds [n]            Show the n most recent keys (default 10).
//   /khoa <key>        Immediately invalidate a key, even if activated.
//   /bokhoa <key>      Undo a revoke.
//   /chitiet <key>     Show everything known about one key.
//   /xoa <key>         Delete a key for good (cannot be undone).
//   /help              List commands.

const TELEGRAM_BOT_TOKEN = Deno.env.get("TELEGRAM_BOT_TOKEN")!;
const ADMIN_IDS = new Set(
  (Deno.env.get("ADMIN_IDS") || "").split(",").map((s) => s.trim()).filter(Boolean),
);
const SUPABASE_URL = Deno.env.get("SUPABASE_URL")!;
const SERVICE_KEY = Deno.env.get("SUPABASE_SERVICE_ROLE_KEY")!;

const TG_API = `https://api.telegram.org/bot${TELEGRAM_BOT_TOKEN}`;

function randomKey(): string {
  // Avoids 0/O/1/I/L so a human typing it from a screenshot doesn't guess wrong.
  const alphabet = "23456789ABCDEFGHJKMNPQRSTUVWXYZ";
  const group = () =>
    Array.from({ length: 4 }, () => alphabet[Math.floor(Math.random() * alphabet.length)]).join("");
  return `HIR-${group()}-${group()}-${group()}`;
}

async function tg(method: string, body: unknown) {
  const res = await fetch(`${TG_API}/${method}`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
  return res.json();
}

function sendMessage(chatId: number | string, text: string) {
  return tg("sendMessage", { chat_id: chatId, text, parse_mode: "HTML" });
}

async function sb(path: string, init: RequestInit & { prefer?: string } = {}) {
  const res = await fetch(`${SUPABASE_URL}/rest/v1/${path}`, {
    ...init,
    headers: {
      apikey: SERVICE_KEY,
      Authorization: `Bearer ${SERVICE_KEY}`,
      "Content-Type": "application/json",
      Prefer: init.prefer ?? "return=representation",
      ...(init.headers ?? {}),
    },
  });
  if (!res.ok) {
    const body = await res.text().catch(() => "");
    throw new Error(`Supabase ${res.status}: ${body}`);
  }
  const text = await res.text();
  return text ? JSON.parse(text) : null;
}

async function cmdNewKey(chatId: number, note: string) {
  const key = randomKey();
  await sb("license_keys", {
    method: "POST",
    body: JSON.stringify({ key, note: note || null, created_by: chatId }),
  });
  await sendMessage(
    chatId,
    `🔑 Key mới:\n<code>${key}</code>${note ? `\nGhi chú: ${note}` : ""}\n\nGửi key này cho khách — key sẽ tự khoá vào máy đầu tiên kích hoạt nó.`,
  );
}

async function cmdList(chatId: number, nArg: string) {
  const n = Math.min(Math.max(parseInt(nArg || "10", 10) || 10, 1), 50);
  const rows = await sb(
    `license_keys?select=key,device_id,revoked,note,customer_name,customer_phone,activated_at,created_at&order=created_at.desc&limit=${n}`,
  );
  if (!rows || rows.length === 0) {
    await sendMessage(chatId, "Chưa có key nào.");
    return;
  }
  type Row = {
    key: string;
    device_id: string | null;
    revoked: boolean;
    note: string | null;
    customer_name: string | null;
    customer_phone: string | null;
  };
  const lines = (rows as Row[]).map((r) => {
    const status = r.revoked ? "❌ đã khoá" : r.device_id ? "🔒 đã kích hoạt" : "⚪ chưa dùng";
    const who = [r.customer_name, r.customer_phone].filter(Boolean).join(" · ");
    const tail = [r.note, who].filter(Boolean).join(" — ");
    return `<code>${r.key}</code> · ${status}${tail ? ` — ${tail}` : ""}`;
  });
  await sendMessage(chatId, lines.join("\n"));
}

async function cmdRevoke(chatId: number, key: string, revoked: boolean) {
  if (!key) {
    await sendMessage(chatId, `Cú pháp: /${revoked ? "khoa" : "bokhoa"} HIR-XXXX-XXXX-XXXX`);
    return;
  }
  const rows = await sb(`license_keys?key=eq.${encodeURIComponent(key)}`, {
    method: "PATCH",
    body: JSON.stringify({ revoked }),
  });
  if (!rows || rows.length === 0) {
    await sendMessage(chatId, `Không tìm thấy key ${key}`);
    return;
  }
  await sendMessage(chatId, `${revoked ? "Đã khoá" : "Đã bỏ khoá"} ${key}`);
}

async function cmdDetail(chatId: number, key: string) {
  if (!key) {
    await sendMessage(chatId, "Cú pháp: /chitiet HIR-XXXX-XXXX-XXXX");
    return;
  }
  const rows = await sb(`license_keys?key=eq.${encodeURIComponent(key)}&select=*`, { method: "GET" });
  if (!rows || rows.length === 0) {
    await sendMessage(chatId, `Không tìm thấy key ${key}`);
    return;
  }
  const r = rows[0];
  const lines = [
    `<code>${r.key}</code>`,
    `Trạng thái: ${r.revoked ? "❌ đã khoá" : r.device_id ? "🔒 đã kích hoạt" : "⚪ chưa dùng"}`,
    `Ghi chú: ${r.note ?? "—"}`,
    `— Khách hàng —`,
    `Họ tên: ${r.customer_name ?? "—"}`,
    `SĐT/Zalo: ${r.customer_phone ?? "—"}`,
    `Gmail: ${r.customer_email ?? "—"}`,
    `Device id: ${r.device_id ?? "—"}`,
    `Kích hoạt lúc: ${r.activated_at ?? "—"}`,
    `Tạo lúc: ${r.created_at}`,
    `Tạo bởi: ${r.created_by ?? "—"}`,
  ];
  await sendMessage(chatId, lines.join("\n"));
}

async function cmdDelete(chatId: number, key: string) {
  if (!key) {
    await sendMessage(chatId, "Cú pháp: /xoa HIR-XXXX-XXXX-XXXX");
    return;
  }
  const rows = await sb(`license_keys?key=eq.${encodeURIComponent(key)}`, { method: "DELETE" });
  if (!rows || rows.length === 0) {
    await sendMessage(chatId, `Không tìm thấy key ${key}`);
    return;
  }
  await sendMessage(chatId, `Đã xoá vĩnh viễn ${key}`);
}

const HELP = [
  "/taomoi [ghi chú] — tạo key mới",
  "/ds [số lượng] — xem key gần đây (mặc định 10)",
  "/khoa <key> — khoá (thu hồi) 1 key",
  "/bokhoa <key> — bỏ khoá",
  "/chitiet <key> — xem đầy đủ thông tin 1 key",
  "/xoa <key> — xoá vĩnh viễn 1 key (không hoàn tác được)",
  "/help — danh sách lệnh",
].join("\n");

Deno.serve(async (req: Request) => {
  if (req.method !== "POST") {
    return new Response("ok");
  }
  let update: any;
  try {
    update = await req.json();
  } catch {
    return new Response("ok");
  }

  const msg = update?.message;
  const chatId = msg?.chat?.id;
  const fromId = String(msg?.from?.id ?? "");
  const text = String(msg?.text ?? "").trim();

  // Always 200 back to Telegram immediately-ish; errors are reported to the
  // admin chat itself, never surfaced as a webhook failure (Telegram would
  // just retry and re-run the same command otherwise).
  if (chatId && text.startsWith("/") && ADMIN_IDS.has(fromId)) {
    const [cmd, ...rest] = text.split(/\s+/);
    const arg = rest.join(" ");
    try {
      if (cmd === "/taomoi") await cmdNewKey(chatId, arg);
      else if (cmd === "/ds") await cmdList(chatId, arg);
      else if (cmd === "/khoa") await cmdRevoke(chatId, arg, true);
      else if (cmd === "/bokhoa") await cmdRevoke(chatId, arg, false);
      else if (cmd === "/chitiet") await cmdDetail(chatId, arg);
      else if (cmd === "/xoa") await cmdDelete(chatId, arg);
      else if (cmd === "/help" || cmd === "/start") await sendMessage(chatId, HELP);
    } catch (e) {
      await sendMessage(chatId, `Lỗi: ${(e as Error).message}`).catch(() => {});
    }
  }

  return new Response("ok");
});
