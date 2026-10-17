import { useEffect, useRef, useState, type ReactNode } from "react";
import { Button, Input } from "@proxyshard/shardx-ui-kit";
import { useT } from "../../shared/i18n";
import { toast } from "../../shared/model/toast";
import { licenseStatus, licenseActivate, licenseInfo, licenseSubmitInfo } from "../../entities/license";

type Step = "checking" | "key" | "info" | "done";

/// How often a running app asks the backend again; the backend itself only
/// contacts the key server when its last good check is older than 6 hours.
const RECHECK_MS = 30 * 60 * 1000;

/// "hlgabcd" → "HLG-ABCD": uppercase and dashes after 3, 7 and 11 chars.
function formatKey(raw: string) {
  const s = raw.toUpperCase().replace(/[^A-Z0-9]/g, "").slice(0, 15);
  return [s.slice(0, 3), s.slice(3, 7), s.slice(7, 11), s.slice(11, 15)].filter(Boolean).join("-");
}

/// Blocks the whole app until this machine has a valid HLG- key (created in
/// @Hirkeys_bot) AND has submitted customer info at least once. A key that
/// gets locked, expires or is deleted in the bot brings this screen back.
export function ActivationGate({ children }: { children: ReactNode }) {
  const t = useT();
  const [step, setStep] = useState<Step>("checking");

  const [key, setKey] = useState("");
  const [name, setName] = useState("");
  const [phone, setPhone] = useState("");
  const [email, setEmail] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const stepRef = useRef(step);
  stepRef.current = step;

  useEffect(() => {
    const check = async () => {
      const activated = await licenseStatus().catch(() => false);
      const info = await licenseInfo().catch(() => null);
      if (!activated) {
        // Key refused on a re-check (locked / expired / deleted): show the
        // key screen again with the old key and the reason filled in — but
        // never overwrite what the user is typing on that screen.
        if (stepRef.current !== "key") {
          if (info?.key) setKey(info.key);
          if (info?.error) setError(info.error);
        }
        setStep("key");
        return;
      }
      setStep((s) => (s === "info" ? s : info?.customer_name ? "done" : "info"));
    };
    check();
    const timer = setInterval(check, RECHECK_MS);
    return () => clearInterval(timer);
  }, []);

  const submitKey = async () => {
    if (!key.trim() || busy) return;
    setBusy(true);
    setError(null);
    try {
      await licenseActivate(key.trim());
      setStep("info");
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const submitInfo = async () => {
    if (!name.trim() || busy) return;
    setBusy(true);
    setError(null);
    try {
      await licenseSubmitInfo(name.trim(), phone.trim(), email.trim());
      toast.ok(t("activationGate.infoSaved"));
      setStep("done");
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  if (step === "checking") {
    return null;
  }
  if (step === "done") {
    return <>{children}</>;
  }

  return (
    <div className="fixed inset-0 z-1000 flex items-center justify-center bg-bg-weak-50 text-text-strong-950">
      <div className="w-[420px] rounded-[14px] bg-bg-white-0 px-9 py-8 text-center shadow-[var(--shadow-md)] ring-1 ring-inset ring-stroke-soft-200">
        <img src="/hir-login-logo.png" alt="" className="mx-auto mb-4 size-12" />

        {step === "key" && (
          <>
            <div className="mb-1.5 text-title-h6">{t("activationGate.title")}</div>
            <p className="m-0 mb-5 text-paragraph-xs text-text-soft-400">
              {t("activationGate.subtitle")}
            </p>
            <div className="flex flex-col gap-3">
              <Input
                inputSize="small"
                className="mono text-center"
                placeholder="HLG-XXXX-XXXX-XXXX"
                value={key}
                onChange={(e) => setKey(formatKey(e.target.value))}
                onKeyDown={(e) => { if (e.key === "Enter") submitKey(); }}
                autoFocus
              />
              {error && <div className="text-paragraph-xs text-error-base">{error}</div>}
              <Button
                variant="primary"
                mode="filled"
                size="small"
                onClick={submitKey}
                disabled={!key.trim() || busy}
                isLoading={busy}
              >
                {busy ? t("activationGate.activating") : t("activationGate.activateBtn")}
              </Button>
            </div>
          </>
        )}

        {step === "info" && (
          <>
            <div className="mb-1.5 text-title-h6">{t("activationGate.infoTitle")}</div>
            <p className="m-0 mb-5 text-paragraph-xs text-text-soft-400">
              {t("activationGate.infoSubtitle")}
            </p>
            <div className="flex flex-col gap-3">
              <Input
                inputSize="small"
                placeholder={t("activationGate.namePlaceholder")}
                value={name}
                onChange={(e) => setName(e.target.value)}
                autoFocus
              />
              <Input
                inputSize="small"
                placeholder={t("activationGate.phonePlaceholder")}
                value={phone}
                onChange={(e) => setPhone(e.target.value)}
              />
              <Input
                inputSize="small"
                placeholder={t("activationGate.emailPlaceholder")}
                value={email}
                onChange={(e) => setEmail(e.target.value)}
                onKeyDown={(e) => { if (e.key === "Enter") submitInfo(); }}
              />
              {error && <div className="text-paragraph-xs text-error-base">{error}</div>}
              <Button
                variant="primary"
                mode="filled"
                size="small"
                onClick={submitInfo}
                disabled={!name.trim() || busy}
                isLoading={busy}
              >
                {busy ? t("activationGate.saving") : t("activationGate.continueBtn")}
              </Button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
