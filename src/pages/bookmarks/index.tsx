import { useEffect, useMemo, useState } from "react";
import { Button, DialogModal, cn } from "@proxyshard/shardx-ui-kit";
import { Topbar } from "../../shared/ui/Topbar";
import { useStoreChanged } from "../../shared/hooks/useStoreChanged";
import { Field } from "../../shared/ui/Field";
import { CSSelect } from "../../shared/ui/CSSelect";
import { AddIcon, DeleteIcon, NavBookmarksIcon } from "../../shared/icons";
import { useBookmarks, emptyBookmark, type Bookmark } from "../../entities/bookmark";
import { useFolders, useProfile } from "../../entities/profile";
import { useT } from "../../shared/i18n";

function Editor({ initial, folders, onClose }: {
  initial: Bookmark;
  folders: string[];
  onClose: () => void;
}) {
  const t = useT();
  const [b, setB] = useState(initial);
  const save = useBookmarks((s) => s.save);
  return (
    <DialogModal
      open
      onClose={onClose}
      title={initial.id ? t("bookmarks.editTitle") : t("bookmarks.newTitle")}
      confirmLabel={t("bookmarks.save")}
      onConfirm={() => save(b)}
      cancelLabel={t("bookmarks.cancel")}
      onCancel={onClose}
    >
      <div className="flex w-[420px] flex-col gap-3 py-4">
        <Field
          label={t("bookmarks.urlLabel")}
          value={b.url}
          onChange={(v) => setB({ ...b, url: v })}
          placeholder="facebook.com/ads/manager"
          mono
        />
        <Field
          label={t("bookmarks.titleLabel")}
          value={b.title}
          onChange={(v) => setB({ ...b, title: v })}
          placeholder={t("bookmarks.titlePlaceholder")}
        />
        <CSSelect
          title={t("bookmarks.folderLabel")}
          value={b.folder}
          onChange={(v) => setB({ ...b, folder: v })}
          isSearchable={folders.length > 8}
          options={[
            { value: "", label: t("bookmarks.folderAnyOption") },
            ...folders.map((f) => ({ value: f, label: f })),
          ]}
        />
        <p className="m-0 text-paragraph-xs text-text-soft-400">
          {t("bookmarks.hintPart1")}<strong>ShardX</strong>{t("bookmarks.hintPart2")}
        </p>
      </div>
    </DialogModal>
  );
}

export function BookmarksPage() {
  const t = useT();
  const init = useBookmarks((s) => s.init);
  const items = useBookmarks((s) => s.items);
  const editing = useBookmarks((s) => s.editing);
  const setEditing = useBookmarks((s) => s.setEditing);
  const remove = useBookmarks((s) => s.remove);
  const search = useBookmarks((s) => s.search);
  const setSearch = useBookmarks((s) => s.setSearch);
  const folder = useBookmarks((s) => s.folder);
  const setFolder = useBookmarks((s) => s.setFolder);

  const initProfiles = useProfile((s) => s.init);
  const folders = useFolders();
  const profiles = useProfile((s) => s.profiles);

  const reload = useBookmarks((s) => s.reload);
  useEffect(() => { init(); initProfiles(); }, [init, initProfiles]);
  useStoreChanged(reload);

  const shown = useMemo(() => {
    const q = search.trim().toLowerCase();
    return items.filter(
      (b) =>
        (folder === "all" || b.folder === (folder === "__any__" ? "" : folder)) &&
        (!q || b.title.toLowerCase().includes(q) || b.url.toLowerCase().includes(q)),
    );
  }, [items, search, folder]);

  const countIn = (f: string) =>
    f === "" ? profiles.length : profiles.filter((p) => p.folder === f).length;

  const tabs = [
    { id: "all", label: t("bookmarks.tabAll") },
    { id: "__any__", label: t("bookmarks.tabEveryProfile") },
    ...folders.map((f) => ({ id: f, label: f })),
  ];

  return (
    <section className="flex flex-col">
      <Topbar crumbs={[t("bookmarks.crumbLibrary"), t("bookmarks.crumbBookmarks")]} search={search} onSearch={setSearch} />

      <div className="mb-3.5 flex items-end justify-between gap-4">
        <div className="flex min-w-0 flex-1 flex-col gap-3.5">
          <div>
            <h1 className="m-0 text-title-h5 text-text-strong-950">{t("bookmarks.heading")}</h1>
            <p className="m-0 mt-1 max-w-[70ch] text-paragraph-xs text-text-soft-400">
              {t("bookmarks.subtitle")}
            </p>
          </div>
          <div className="flex flex-wrap gap-1.5">
            {tabs.map((tab) => (
              <button
                key={tab.id}
                type="button"
                onClick={() => setFolder(tab.id)}
                className={cn(
                  "rounded-6 px-2.5 py-1 text-label-xs ring-1 ring-inset transition-colors",
                  folder === tab.id
                    ? "bg-primary-alpha-10 text-primary-base ring-primary-alpha-24"
                    : "text-text-sub-600 ring-stroke-soft-200 hover:bg-bg-weak-50",
                )}
              >
                {tab.label}
              </button>
            ))}
          </div>
        </div>
        <Button
          variant="primary" mode="filled" size="small"
          leftIcon={<AddIcon className="size-4" />}
          onClick={() => setEditing(emptyBookmark(folder === "all" || folder === "__any__" ? "" : folder))}
        >
          {t("bookmarks.newButton")}
        </Button>
      </div>

      <div className="overflow-hidden rounded-12 bg-bg-white-0 shadow-[var(--shadow-xs)] ring-1 ring-inset ring-stroke-soft-200">
        {shown.map((b) => (
          <div
            key={b.id}
            className="grid grid-cols-[1fr_1fr_200px_100px] items-center gap-3 border-t border-stroke-soft-200 px-4 py-2.5 first:border-t-0 transition-colors hover:bg-bg-weak-50"
          >
            <div
              className="min-w-0 cursor-pointer truncate text-label-xs text-text-strong-950 transition-colors hover:text-primary-base"
              onClick={() => setEditing(b)}
              title={t("bookmarks.editTooltip")}
            >
              {b.title || b.url}
            </div>
            <div className="mono min-w-0 truncate text-paragraph-xs text-text-sub-600" title={b.url}>
              {b.url}
            </div>
            <div className="truncate text-paragraph-xs text-text-sub-600">
              {b.folder
                ? countIn(b.folder) === 1
                  ? t("bookmarks.folderProfileCountOne", { folder: b.folder })
                  : t("bookmarks.folderProfileCount", { folder: b.folder, n: countIn(b.folder) })
                : t("bookmarks.everyProfileCount", { n: profiles.length })}
            </div>
            <div className="flex justify-end">
              <Button
                variant="error" mode="ghost" size="2xsmall"
                leftIcon={<DeleteIcon className="size-3.5" />}
                onClick={() => remove(b)}
              >
                {t("bookmarks.delete")}
              </Button>
            </div>
          </div>
        ))}
        {shown.length === 0 && (
          <div className="flex flex-col items-center gap-2.5 px-6 py-14 text-center">
            <div className="grid size-14 place-items-center rounded-[14px] bg-primary-alpha-10 text-primary-base ring-1 ring-inset ring-primary-alpha-24">
              <NavBookmarksIcon className="size-6" />
            </div>
            <h3 className="m-0 text-label-sm text-text-strong-950">
              {items.length === 0 ? t("bookmarks.emptyTitle") : t("bookmarks.emptyFolderTitle")}
            </h3>
            <p className="m-0 max-w-[420px] text-paragraph-sm text-text-sub-600">
              {t("bookmarks.emptyHint")}
            </p>
          </div>
        )}
      </div>

      {editing && (
        <Editor initial={editing} folders={folders} onClose={() => setEditing(null)} />
      )}
    </section>
  );
}
