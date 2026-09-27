import { useEffect, useMemo, useState } from "react";
import { teamCall, useTeam } from "../../../shared/model/teamRole";
import { useFolders } from "./selectors";

/** Folders a profile may be put in. On a team, an admin sees all of them and a
 *  group manager only those they manage; off a team it is the local folders. */
export function useFolderChoices(current = ""): string[] {
  const local = useFolders();
  const role = useTeam((s) => s.role);
  const [server, setServer] = useState<string[]>([]);
  useEffect(() => {
    if (role !== "admin" && role !== "manager") { setServer([]); return; }
    teamCall<{ folders: { name: string }[] }>("GET", "/admin/folders")
      .then((r) => setServer(r.folders.map((f) => f.name)))
      .catch(() => setServer([]));
  }, [role]);
  return useMemo(() => {
    const set = new Set<string>(role === "manager" ? server : [...local, ...server]);
    if (current) set.add(current);
    return [...set].sort((a, b) => a.localeCompare(b));
  }, [local, server, role, current]);
}
