import { Button } from "@proxyshard/shardx-ui-kit";
import {
  PinIconApp,
  EditIcon,
  CopyIcon,
  DeleteIcon,
  MoreIcon,
  PlayIcon,
  StopIcon,
} from "../../../shared/icons";
import { useProfile, type ProfileMeta } from "../../../entities/profile";
import { useT } from "../../../shared/i18n";

export function ProfileRowActions({ profile, onMore }: {
  profile: ProfileMeta;
  onMore: (e: React.MouseEvent) => void;
}) {
  const t = useT();
  const p = profile;
  const isRunning = useProfile((s) => !!s.running[p.id]);
  const isStarting = useProfile((s) => s.startBusy.has(p.id));
  const startStop = useProfile((s) => s.startStop);
  const togglePin = useProfile((s) => s.togglePin);
  const cloneProfile = useProfile((s) => s.cloneProfile);
  const remove = useProfile((s) => s.remove);
  const expand = useProfile((s) => s.expand);

  return (
    <div className="flex justify-end gap-1">
      <Button
        variant={isRunning ? "error" : "primary"}
        mode="lighter"
        size="xsmall"
        fullRadius
        className="min-w-[84px]"
        leftIcon={
          isRunning
            ? <StopIcon className="size-3.5" />
            : <span className={isStarting ? "spin-icon inline-grid place-items-center" : "inline-grid place-items-center"}><PlayIcon className="size-3.5" /></span>
        }
        onClick={() => startStop(p)}
        disabled={!isRunning && isStarting}
        title={!isRunning && isStarting ? t("profileRowActions.startingTitle") : undefined}
      >
        {isRunning ? t("profileRowActions.stop") : isStarting ? t("profileRowActions.starting") : t("profileRowActions.start")}
      </Button>
      <Button
        variant={p.pinned ? "primary" : "neutral"}
        mode={p.pinned ? "lighter" : "stroke"}
        size="xsmall"
        onlyIcon
        onClick={() => togglePin(p)}
        title={p.pinned ? t("profileRowActions.unpin") : t("profileRowActions.pinToTop")}
        leftIcon={<PinIconApp className="size-4" />}
      >
      
      </Button>
      <Button variant="neutral" mode="stroke" size="xsmall" onlyIcon onClick={() => expand(p.id)} title={t("profileRowActions.edit")}
        leftIcon={<EditIcon className="size-4" />}
      >
      </Button>
      <Button variant="neutral" mode="stroke" size="xsmall" onlyIcon onClick={() => cloneProfile(p.id)} title={t("profileRowActions.clone")}
        leftIcon={<CopyIcon className="size-4" />}
      >
      </Button>
      <Button variant="error" mode='filled' size="xsmall" onlyIcon onClick={() => remove(p.id)} title={t("profileRowActions.delete")}
        leftIcon={<DeleteIcon className="size-4" />}
      >
      </Button>
      <Button
        variant="neutral"
        mode="stroke"
        size="xsmall"
        onlyIcon
        onClick={onMore}
        title={t("profileRowActions.moreActions")}
        leftIcon={<MoreIcon className="size-4" />}
      >
      </Button>
    </div>
  );
}
