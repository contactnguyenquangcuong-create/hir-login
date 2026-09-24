# Hir-Login license key system

Three parts:

1. **`supabase.sql`** — run once in your Supabase project's SQL editor. Creates
   the `license_keys` table (locked down — no direct access from the app) and
   an `activate_license` RPC the app calls to redeem a key.
2. **The Telegram bot** — admin-only, creates/lists/revokes keys. Two ways to
   run it; pick one:

   ### Option A — Supabase Edge Function (recommended, no VPS)
   `supabase/functions/telegram-bot/index.ts`. Telegram pushes each message
   straight to Supabase over a webhook; Supabase hosts the function itself,
   so nothing needs to stay running on your own machine.

   ```bash
   # once: install the CLI and log in
   npm install -g supabase
   supabase login
   supabase link --project-ref myyqyillsgizshrvoncs

   # deploy the function (--no-verify-jwt: Telegram doesn't send a Supabase JWT)
   cd license-system
   supabase functions deploy telegram-bot --no-verify-jwt

   # secrets the function reads (SUPABASE_URL / SUPABASE_SERVICE_ROLE_KEY are
   # injected automatically by Supabase — do not set those yourself)
   supabase secrets set TELEGRAM_BOT_TOKEN=123:abc ADMIN_IDS=123456789

   # tell Telegram where to deliver messages (run once)
   curl "https://api.telegram.org/bot<TOKEN>/setWebhook?url=https://myyqyillsgizshrvoncs.supabase.co/functions/v1/telegram-bot"
   ```

   To check the webhook is registered: `curl https://api.telegram.org/bot<TOKEN>/getWebhookInfo`.

   ### Option B — plain Node.js script (needs an always-on machine)
   `bot.js`, long-polls Telegram instead of using a webhook — simpler to read,
   but something has to keep the process alive (a VPS, `pm2`, systemd, a
   Raspberry Pi — closing the terminal or sleeping the laptop stops it):

   ```bash
   TELEGRAM_BOT_TOKEN=123:abc \
   SUPABASE_URL=https://xxxx.supabase.co \
   SUPABASE_SERVICE_ROLE_KEY=eyJ... \
   ADMIN_IDS=123456789 \
   node bot.js
   ```

   Either way:
   - `TELEGRAM_BOT_TOKEN`: from [@BotFather](https://t.me/BotFather) (`/newbot`).
   - `SUPABASE_SERVICE_ROLE_KEY`: Project Settings → API. **Admin-level DB
     access — keep it only in the bot (Edge Function secret or your own env),
     never in the Hir-Login app.**
   - `ADMIN_IDS`: comma-separated Telegram numeric user IDs allowed to run
     commands (DM [@userinfobot](https://t.me/userinfobot) to find your own id).

   Command menu shown in Telegram's "/" button — set once:
   ```bash
   curl -X POST "https://api.telegram.org/bot<TOKEN>/setMyCommands" \
     -H "Content-Type: application/json" \
     -d '{"commands":[
       {"command":"newkey","description":"Tạo key bản quyền mới (vd: /newkey khach-A)"},
       {"command":"list","description":"Xem danh sách key gần đây (vd: /list 20)"},
       {"command":"revoke","description":"Thu hồi 1 key (vd: /revoke HIR-XXXX-XXXX-XXXX)"},
       {"command":"unrevoke","description":"Bỏ thu hồi 1 key đã bị khoá"},
       {"command":"help","description":"Xem hướng dẫn sử dụng bot"}
     ]}'
   ```

3. **App side** (already wired into Hir-Login): on first launch, the app asks
   for a key, calls `activate_license` with the key and this machine's
   device id (a hash derived from hardware, not the raw serial), and only
   the **anon** key is embedded in the app — it can only call that one RPC,
   never read the table directly. Once a key binds to a device id it stays
   offline-valid on that machine; a different machine trying the same key is
   refused with "in use".

## Telegram commands

```
/newkey [note]     Create a key, e.g. /newkey khach-A
/list [n]          Show the n most recent keys (default 10)
/revoke <key>      Invalidate a key immediately, even if already activated
/unrevoke <key>    Undo a revoke
/help              List commands
```

## Config already set in the Hir-Login app

`src-tauri/src/license.rs` has `SUPABASE_URL` and the **anon** public key
baked in — nothing left to configure there.
