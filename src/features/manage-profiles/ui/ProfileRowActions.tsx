import { Button } from "@proxyshard/shardx-ui-kit";
import {
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
  // The browser has closed and the close is still on its way to the team: nothing to start or stop
  // here, and no other machine can open the profile until it is through.
  const isSaving = useProfile((s) => s.syncing.has(p.id));
  const elsewhere = useProfile((s) => s.elsewhere[p.id]);
  const startStop = useProfile((s) => s.startStop);

  return (
    <div className="flex justify-end gap-1">
      {!isRunning && isSaving && !isStarting ? (
        <Button
          variant="neutral"
          mode="lighter"
          size="xsmall"
          fullRadius
          className="w-[120px] flex-none"
          leftIcon={<span className="spin-icon inline-grid place-items-center"><PlayIcon className="size-3.5" /></span>}
          disabled
          title={t("profileRowActions.syncingTitle")}
        >
          {t("profileRowActions.syncing")}
        </Button>
      ) : !isRunning && elsewhere ? (
        <Button
          variant="neutral"
          mode="lighter"
          size="xsmall"
          fullRadius
          className="w-[120px] max-w-[120px] flex-none justify-start overflow-hidden"
          leftIcon={elsewhere.saving ? <span className="spin-icon inline-grid place-items-center"><PlayIcon className="size-3.5" /></span> : <PlayIcon className="size-3.5" />}
          disabled
          title={elsewhere.saving ? t("profileRowActions.savedByTitle", { who: elsewhere.by }) : t("profileRowActions.usedByTitle", { who: elsewhere.by })}
        >
          <span className="min-w-0 truncate">{elsewhere.saving ? t("profileRowActions.savedBy", { who: elsewhere.by }) : t("profileRowActions.usedBy", { who: elsewhere.by })}</span>
        </Button>
      ) : (
        <Button
          variant={isRunning ? "error" : "primary"}
          mode="lighter"
          size="xsmall"
          fullRadius
          className="w-[120px] flex-none"
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
      )}
      {/* Pin, edit, clone and delete are in the ⋮ menu (and on right-click): five icon buttons
          on every row were most of the table's width and most of its noise. */}
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
