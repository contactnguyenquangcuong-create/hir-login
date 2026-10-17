export type LicenseInfo = {
  key: string;
  device_id: string;
  activated_at: string;
  /** ISO 8601 from the key server; null = permanent. */
  expires_at: string | null;
  /** The admin's note on the key in @Hirkeys_bot. */
  note: string;
  /** Why the server refused the key on the last re-check (locked, expired…). */
  error: string | null;
  customer_name: string | null;
  customer_phone: string | null;
  customer_email: string | null;
};
