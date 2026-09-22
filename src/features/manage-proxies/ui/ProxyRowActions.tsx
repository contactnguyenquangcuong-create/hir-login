import { Button } from "@proxyshard/shardx-ui-kit";
import {
  InfoIcon,
  RefreshIcon,
  EditIcon,
  DeleteIcon,
} from "../../../shared/icons";
import { useProxy, type ProxyEntry } from "../../../entities/proxy";
import { useT } from "../../../shared/i18n";

export function ProxyRowActions({ proxy }: { proxy: ProxyEntry }) {
  const t = useT();
  const busy = useProxy((s) => !!s.proxyTesting[proxy.id]);
  const testProxy = useProxy((s) => s.testProxy);
  const removeProxy = useProxy((s) => s.removeProxy);
  const setEditing = useProxy((s) => s.setEditing);
  const setInfoFor = useProxy((s) => s.setInfoFor);
  const isInfoOpen = useProxy((s) => s.infoFor?.proxy.id === proxy.id);

  return (
    <div className="flex justify-end gap-1">
      <Button
        variant="neutral"
        mode="stroke"
        size="xsmall"
        onMouseDown={(e) => e.stopPropagation()}
        onClick={(e) =>
          isInfoOpen
            ? setInfoFor(null)
            : setInfoFor({ proxy, anchor: { x: e.clientX, y: e.clientY } })
        }
        title={t("proxyRowActions.detailsTitle")}
        leftIcon={<InfoIcon />}
      >
      </Button>
      <Button variant="neutral" mode="stroke" size="xsmall"  onlyIcon onClick={() => testProxy(proxy)} disabled={busy} title={t("proxyRowActions.testTitle")}
        leftIcon={<RefreshIcon />}
      >
      </Button>
      <Button variant="neutral" mode="stroke" size="xsmall"  onlyIcon onClick={() => setEditing(proxy)} title={t("proxyRowActions.editTitle")}
        leftIcon={<EditIcon />}
      >
      </Button>
      <Button variant="error" mode='filled' size="xsmall"  onlyIcon onClick={() => removeProxy(proxy.id)} title={t("proxyRowActions.deleteTitle")}
        leftIcon={<DeleteIcon />}
      >
      </Button>
    </div>
  );
}
