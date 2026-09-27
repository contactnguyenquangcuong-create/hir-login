// Hir-Login license-key admin bot — zero-dependency, single file.
// Generates/revokes/lists license keys in Supabase. Admin-only (checked
// against ADMIN_IDS); everyone else is ignored.
//
// Run:
//   TELEGRAM_BOT_TOKEN=123:abc \
//   SUPABASE_URL=https://xxxx.supabase.co \
//   SUPABASE_SERVICE_ROLE_KEY=eyJ... \
//   ADMIN_IDS=123456789,987654321 \
//   node bot.js
//
// The service_role key gives full DB access, bypassing every RLS policy —
// keep it only here, on a machine/server you control. NEVER put it in the
// Hir-Login app itself; the app only ever holds the public anon key and
// calls the activate_license RPC (see supabase.sql).
//
// Commands (DM the bot, admin only):
//   /taomoi [số lượng] [ghi chú]  Create 1 key or N at once (max 100), optionally tagged (e.g. a
//                      customer name). Replies with the key.
//   /ds [n]            Show the n most recent keys (default 10).
//   /khoa <key>        Immediately invalidate a key, even if activated.
//   /bokhoa <key>      Undo a revoke.
//   /chitiet <key>     Show everything known about one key.
//   /xoa <key>         Delete a key for good (cannot be undone).
//   /help              List commands.

const TOKEN = process.env.TELEGRAM_BOT_TOKEN;
const SUPABASE_URL = (process.env.SUPABASE_URL || "").replace(/\/+$/, "");
const SERVICE_KEY = process.env.SUPABASE_SERVICE_ROLE_KEY;
const ADMIN_IDS = new Set(
  (process.env.ADMIN_IDS || "")
    .split(",")
    .map((s) => s.trim())
    .filter(Boolean),
);

if (!TOKEN || !SUPABASE_URL || !SERVICE_KEY || ADMIN_IDS.size === 0) {
  console.error(
    "Missing env vars — need TELEGRAM_BOT_TOKEN, SUPABASE_URL, SUPABASE_SERVICE_ROLE_KEY, ADMIN_IDS",
  );
  process.exit(1);
}

const TG_API = `https://api.telegram.org/bot${TOKEN}`;

// Telegram caps a message at 4096 characters; 100 keys stay well under it.
const MAX_KEYS_PER_CALL = 100;

function randomKey() {
  // Avoids 0/O/1/I/L so a human typing it from a screenshot doesn't guess wrong.
  const alphabet = "23456789ABCDEFGHJKMNPQRSTUVWXYZ";
  const group = () =>
    Array.from({ length: 4 }, () => alphabet[Math.floor(Math.random() * alphabet.length)]).join("");
  return `HIR-${group()}-${group()}-${group()}`;
}

async function tg(method, body) {
  const res = await fetch(`${TG_API}/${method}`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
  return res.json();
}

function sendMessage(chatId, text) {
  return tg("sendMessage", { chat_id: chatId, text, parse_mode: "HTML" });
}

async function sb(path, init = {}) {
  const res = await fetch(`${SUPABASE_URL}/rest/v1/${path}`, {
    ...init,
    headers: {
      apikey: SERVICE_KEY,
      Authorization: `Bearer ${SERVICE_KEY}`,
      "Content-Type": "application/json",
      Prefer: init.prefer || "return=representation",
      ...(init.headers || {}),
    },
  });
  if (!res.ok) {
    const body = await res.text().catch(() => "");
    throw new Error(`Supabase ${res.status}: ${body}`);
  }
  const text = await res.text();
  return text ? JSON.parse(text) : null;
}

// `/taomoi` = 1 key, `/taomoi 15` = 15 keys, `/taomoi 15 ghi chú` or `/taomoi ghi chú` add a note.
async function cmdNewKey(chatId, arg) {
  const m = String(arg || "").trim().match(/^(\d+)(?:\s+(.*))?$/);
  const count = m ? Math.min(Math.max(parseInt(m[1], 10), 1), MAX_KEYS_PER_CALL) : 1;
  const note = (m ? (m[2] ?? "") : String(arg || "")).trim();
  if (m && parseInt(m[1], 10) > MAX_KEYS_PER_CALL) {
    await sendMessage(chatId, `Tối đa ${MAX_KEYS_PER_CALL} key mỗi lần — sẽ tạo ${MAX_KEYS_PER_CALL} key.`);
  }
  const keys = new Set();
  while (keys.size < count) keys.add(randomKey());
  const list = [...keys];
  await sb("license_keys", {
    method: "POST",
    body: JSON.stringify(list.map((key) => ({ key, note: note || null, created_by: Number(chatId) }))),
  });
  const noteLine = note ? `\nGhi chú: ${note}` : "";
  const tail = "\n\nGửi key cho khách — key tự khoá vào máy đầu tiên kích hoạt nó.";
  if (list.length === 1) {
    await sendMessage(chatId, `🔑 Key mới:\n<code>${list[0]}</code>${noteLine}${tail}`);
    return;
  }
  await sendMessage(
    chatId,
    `🔑 Đã tạo ${list.length} key mới:\n${list.map((k) => `<code>${k}</code>`).join("\n")}${noteLine}${tail}`,
  );
}

async function cmdList(chatId, nArg) {
  const n = Math.min(Math.max(parseInt(nArg || "10", 10) || 10, 1), 50);
  const rows = await sb(
    `license_keys?select=key,device_id,revoked,note,customer_name,customer_phone,activated_at,created_at&order=created_at.desc&limit=${n}`,
  );
  if (!rows || rows.length === 0) {
    await sendMessage(chatId, "Chưa có key nào.");
    return;
  }
  const lines = rows.map((r) => {
    const status = r.revoked ? "❌ đã khoá" : r.device_id ? "🔒 đã kích hoạt" : "⚪ chưa dùng";
    const who = [r.customer_name, r.customer_phone].filter(Boolean).join(" · ");
    const tail = [r.note, who].filter(Boolean).join(" — ");
    return `<code>${r.key}</code> · ${status}${tail ? ` — ${tail}` : ""}`;
  });
  await sendMessage(chatId, lines.join("\n"));
}

async function cmdRevoke(chatId, key, revoked) {
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

async function cmdDetail(chatId, key) {
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

async function cmdDelete(chatId, key) {
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
  "/taomoi [số lượng] [ghi chú] — tạo key mới (vd /taomoi 15 = tạo 15 key)",
  "/ds [số lượng] — xem key gần đây (mặc định 10)",
  "/khoa <key> — khoá (thu hồi) 1 key",
  "/bokhoa <key> — bỏ khoá",
  "/chitiet <key> — xem đầy đủ thông tin 1 key",
  "/xoa <key> — xoá vĩnh viễn 1 key (không hoàn tác được)",
  "/help — danh sách lệnh",
].join("\n");

async function handleMessage(msg) {
  const chatId = msg.chat?.id;
  const fromId = String(msg.from?.id ?? "");
  const text = (msg.text || "").trim();
  if (!chatId || !text.startsWith("/")) return;

  if (!ADMIN_IDS.has(fromId)) {
    return; // Silently ignore non-admins.
  }

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
    await sendMessage(chatId, `Lỗi: ${e.message}`);
  }
}

async function pollLoop() {
  let offset = 0;
  console.log("Hir-Login license bot started, polling Telegram…");
  for (;;) {
    try {
      const res = await tg("getUpdates", { offset, timeout: 30 });
      for (const update of res.result || []) {
        offset = update.update_id + 1;
        if (update.message) handleMessage(update.message).catch((e) => console.error(e));
      }
    } catch (e) {
      console.error("poll error:", e.message);
      await new Promise((r) => setTimeout(r, 3000));
    }
  }
}

pollLoop();
