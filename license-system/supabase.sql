-- Hir-Login license key system.
-- Run this once in the Supabase project's SQL editor (https://supabase.com/dashboard/project/_/sql/new).
--
-- Design:
--   * The `license_keys` table is NEVER readable/writable directly by the
--     app (RLS blocks the anon role completely). The app only ever calls
--     the `activate_license` RPC below, which runs as the table owner
--     (SECURITY DEFINER) and returns nothing but {ok, error}.
--   * The Telegram bot uses the service_role key (full access, bypasses
--     RLS) to create/revoke/list keys directly. The service_role key must
--     NEVER be embedded in the Hir-Login app itself — only the bot process
--     holds it.
--   * A key binds to exactly one device_id, forever, on first successful
--     activation. Re-activating from the SAME device_id (e.g. app data was
--     wiped) still succeeds. Any OTHER device_id trying the same key is
--     refused with "in_use".

create table if not exists public.license_keys (
  key text primary key,
  device_id text,
  activated_at timestamptz,
  revoked boolean not null default false,
  note text,
  created_by bigint,           -- Telegram user id of the admin who made it
  created_at timestamptz not null default now()
);

alter table public.license_keys enable row level security;
-- Deliberately no policies: anon and authenticated have zero direct access.
-- Only service_role (the bot) and the SECURITY DEFINER function below can
-- touch this table.

create or replace function public.activate_license(p_key text, p_device_id text)
returns jsonb
language plpgsql
security definer
set search_path = public
as $$
declare
  rec public.license_keys%rowtype;
begin
  if p_key is null or length(trim(p_key)) = 0 then
    return jsonb_build_object('ok', false, 'error', 'empty_key');
  end if;
  if p_device_id is null or length(trim(p_device_id)) = 0 then
    return jsonb_build_object('ok', false, 'error', 'empty_device');
  end if;

  select * into rec from public.license_keys where key = p_key for update;

  if not found then
    return jsonb_build_object('ok', false, 'error', 'not_found');
  end if;

  if rec.revoked then
    return jsonb_build_object('ok', false, 'error', 'revoked');
  end if;

  if rec.device_id is not null and rec.device_id <> p_device_id then
    return jsonb_build_object('ok', false, 'error', 'in_use');
  end if;

  if rec.device_id is null then
    update public.license_keys
      set device_id = p_device_id, activated_at = now()
      where key = p_key;
  end if;

  return jsonb_build_object('ok', true);
end;
$$;

-- The anon key (embedded in the shipped app) may call this RPC and nothing else.
revoke all on function public.activate_license(text, text) from public;
grant execute on function public.activate_license(text, text) to anon;
grant execute on function public.activate_license(text, text) to authenticated;

-- ---- Customer info, collected right after activation ----
-- Safe to run again on an existing table: adds the columns only if missing.
alter table public.license_keys add column if not exists customer_name text;
alter table public.license_keys add column if not exists customer_phone text;
alter table public.license_keys add column if not exists customer_email text;
alter table public.license_keys add column if not exists info_updated_at timestamptz;

-- Only the device that holds the activation for this key may write its own
-- customer info — both key AND device_id must match the bound record.
create or replace function public.submit_customer_info(
  p_key text, p_device_id text, p_name text, p_phone text, p_email text
)
returns jsonb
language plpgsql
security definer
set search_path = public
as $$
declare
  rec public.license_keys%rowtype;
begin
  select * into rec from public.license_keys where key = p_key for update;

  if not found then
    return jsonb_build_object('ok', false, 'error', 'not_found');
  end if;
  if rec.revoked then
    return jsonb_build_object('ok', false, 'error', 'revoked');
  end if;
  if rec.device_id is distinct from p_device_id then
    return jsonb_build_object('ok', false, 'error', 'in_use');
  end if;

  update public.license_keys
    set customer_name = nullif(trim(p_name), ''),
        customer_phone = nullif(trim(p_phone), ''),
        customer_email = nullif(trim(p_email), ''),
        info_updated_at = now()
    where key = p_key;

  return jsonb_build_object('ok', true);
end;
$$;

revoke all on function public.submit_customer_info(text, text, text, text, text) from public;
grant execute on function public.submit_customer_info(text, text, text, text, text) to anon;
grant execute on function public.submit_customer_info(text, text, text, text, text) to authenticated;

-- Lets the Settings page re-fetch this machine's own record (key + device_id
-- must both match — that pair is only ever known to the activated machine).
create or replace function public.get_license_info(p_key text, p_device_id text)
returns jsonb
language plpgsql
security definer
set search_path = public
as $$
declare
  rec public.license_keys%rowtype;
begin
  select * into rec from public.license_keys
    where key = p_key and device_id = p_device_id;

  if not found then
    return jsonb_build_object('ok', false, 'error', 'not_found');
  end if;

  return jsonb_build_object(
    'ok', true,
    'key', rec.key,
    'device_id', rec.device_id,
    'activated_at', rec.activated_at,
    'customer_name', rec.customer_name,
    'customer_phone', rec.customer_phone,
    'customer_email', rec.customer_email,
    'note', rec.note
  );
end;
$$;

revoke all on function public.get_license_info(text, text) from public;
grant execute on function public.get_license_info(text, text) to anon;
grant execute on function public.get_license_info(text, text) to authenticated;
