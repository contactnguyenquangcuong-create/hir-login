import { SegmentControl, useTheme } from "@proxyshard/shardx-ui-kit";
import { SunIcon, MoonIcon } from "../../shared/icons";
import { useT } from "../../shared/i18n";

/// Light/dark switch — UI-kit SegmentControl bound to the kit ThemeProvider.
export function ThemeSwitch() {
  const t = useT();
  const { resolvedTheme, setTheme } = useTheme();
  return (
    <SegmentControl
      size="small"
      className="mb-2 w-full *:flex-1"
      value={resolvedTheme}
      items={[
        { value: "light", label: t("themeSwitch.light"), icon: <SunIcon className="size-4" /> },
        { value: "dark", label: t("themeSwitch.dark"), icon: <MoonIcon className="size-4" /> },
      ]}
      onChange={(v) => setTheme(v as "light" | "dark")}
    />
  );
}
