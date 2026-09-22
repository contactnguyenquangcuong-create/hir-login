import Badge from "../../../shared/ui/Badge";
import type { ProxyEntry } from "../model/types";

export function ProxyTypeBadge({ kind }: { kind: ProxyEntry["kind"] }) {
  return (
    <Badge
      size="small"
      variant='filled'
      color={kind === "socks5" ? "primary" : kind === "https" ? "success" : 'gray'}
    >
      {kind.toUpperCase()}
    </Badge>
  );
}
