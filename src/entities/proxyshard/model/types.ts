export type ResiType = "standart" | "premium" | "unmetered";

export type PsMe = { email: string; active_orders: number; wallet_balance: number };
export type PsOrder = {
  order_id: number;
  product_name: string;
  cycle_name: string;
  expires_at: string | null;
  auto_renewal: boolean | null;
  tag: string | null;
};
export type PsProduct = { name: string; description?: string | null; location?: string | null; cycles: string[] };
export type PsCalc = { original_price: number; final_price: number; discount_percent: number; addons_price?: number; total_with_addons?: number };
export type PsLoc = { code: string; name: string };
export type PsActiveProxy = { ip: string; username: string; password: string; http_port: number; socks_port: number; until: string; status: string; signature?: string | null };
export type PsBuyOption = { name: string; cycles: string[]; locations: string[] };
