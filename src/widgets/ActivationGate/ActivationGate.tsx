import { useEffect, useState, type ReactNode } from "react";
import { Button, Input } from "@proxyshard/shardx-ui-kit";
import { useT } from "../../shared/i18n";
import { toast } from "../../shared/model/toast";
import { licenseStatus, licenseActivate, licenseInfo, licenseSubmitInfo } from "../../entities/license";

type Step = "checking" | "key" | "info" | "done";

/// Blocks the whole app until this machine has a valid local activation
/// AND has submitted customer info at least once. Checked once at startup —
/// no network call unless activation/info is actually missing.
export function ActivationGate({ children }: { children: ReactNode }) {
  const t = useT();
  const [step, setStep] = useState<Step>("checking");

  const [key, setKey] = useState("");
  const [name, setName] = useState("");
  const [phone, setPhone] = useState("");
  const [email, setEmail] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    (async () => {
      const activated = await licenseStatus().catch(() => false);
      if (!activated) {
        setStep("key");
        return;
      }
      const info = await licenseInfo().catch(() => null);
      setStep(info?.customer_name ? "done" : "info");
    })();
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
                placeholder="HIR-XXXX-XXXX-XXXX"
                value={key}
                onChange={(e) => setKey(e.target.value.toUpperCase())}
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
