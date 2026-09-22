import { useEffect, useState } from 'react'
import { createPortal } from 'react-dom'
import { cn } from '@/lib/cn'
import Button from '@/components/Button'
import Checkbox from '@/components/Checkbox'
import type { ButtonVariant } from '@/components/Button'
import { HugeiconsIcon } from '@hugeicons/react'
import {
  AlertCircleIcon,
  Alert02Icon,
  CheckmarkCircle02Icon,
  InformationCircleIcon,
  AiMagicIcon,
} from '@hugeicons/core-free-icons'

export type AlertModalStatus =
  | 'feature'
  | 'information'
  | 'success'
  | 'warning'
  | 'error'

export type AlertModalProps = {
  open: boolean
  onClose: () => void
  status?: AlertModalStatus
  /** Override the default status icon. */
  icon?: React.ReactNode
  title: React.ReactNode
  description?: React.ReactNode

  /** Primary/confirm action. */
  confirmLabel?: string
  onConfirm?: () => void
  confirmVariant?: ButtonVariant
  isLoading?: boolean

  /** Secondary/cancel action. Hidden when `hideCancel`. */
  cancelLabel?: string
  onCancel?: () => void
  hideCancel?: boolean

  /** Situational "Don't show it again" checkbox. */
  showDontShowAgain?: boolean
  dontShowAgain?: boolean
  onDontShowAgainChange?: (checked: boolean) => void
  dontShowAgainLabel?: string

  className?: string
}



// static class maps (no dynamic class names → visible to Tailwind / @source)
const badge: Record<AlertModalStatus, { bg: string; icon: string }> = {
  feature: { bg: 'bg-[#6F42C129]', icon: 'text-primary-base' },
  information: { bg: 'bg-[#EBF1FF]', icon: 'text-information-base' },
  success: { bg: 'bg-[#1FC16B29]', icon: 'text-success-base' },
  warning: { bg: 'bg-[#FF990029]', icon: 'text-warning-base' },
  error: { bg: 'bg-[#FB374829]', icon: 'text-error-base' },
}

const statusIcon: Record<AlertModalStatus, React.ReactNode> = {
  feature: <HugeiconsIcon icon={AiMagicIcon} className="size-5" />,
  information: <HugeiconsIcon icon={InformationCircleIcon} className="size-5" />,
  success: <HugeiconsIcon icon={CheckmarkCircle02Icon} className="size-5" />,
  warning: <HugeiconsIcon icon={Alert02Icon} className="size-5" />,
  error: <HugeiconsIcon icon={AlertCircleIcon} className="size-5" />,
}

export default function AlertModal({
  open,
  onClose,
  status = 'feature',
  icon,
  title,
  description,
  confirmLabel = 'Continue',
  onConfirm,
  confirmVariant,
  isLoading = false,
  cancelLabel = 'Cancel',
  onCancel,
  hideCancel = false,
  showDontShowAgain = false,
  dontShowAgain = false,
  onDontShowAgainChange,
  dontShowAgainLabel = "Don't show it again",
  className,
}: AlertModalProps) {
  const [mounted, setMounted] = useState(false)
  const [render, setRender] = useState(false)
  const [visible, setVisible] = useState(false)

  useEffect(() => setMounted(true), [])

  useEffect(() => {
    if (open) {
      setRender(true)
      requestAnimationFrame(() => requestAnimationFrame(() => setVisible(true)))
    } else {
      setVisible(false)
    }
  }, [open])

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === 'Escape' && onClose()
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [onClose])

  useEffect(() => {
    document.body.style.overflow = open ? 'hidden' : ''
    return () => {
      document.body.style.overflow = ''
    }
  }, [open])

  if (!mounted || !render) return null

  const p = badge[status]
  const resolvedConfirmVariant: ButtonVariant =
    confirmVariant ?? (status === 'error' ? 'error' : 'primary')
  const target = document.getElementById('portal-root') ?? document.body

  return createPortal(
    <div
      className="fixed inset-0 z-50 flex items-center justify-center p-4 transition-colors duration-200"
      style={{ backgroundColor: visible ? 'var(--backdrop)' : 'transparent' }}
      onClick={onClose}
      onTransitionEnd={() => {
        if (!open) setRender(false)
      }}
    >
      <div
        role="alertdialog"
        aria-modal="true"
        onClick={(e) => e.stopPropagation()}
        className={cn(
          'flex w-[440px] max-w-[calc(100vw-2rem)] flex-col overflow-hidden rounded-[20px] border border-stroke-soft-200 bg-bg-white-0 shadow-[var(--shadow-md)] transition-all duration-200',
          className,
        )}
        style={{
          opacity: visible ? 1 : 0,
          transform: visible ? 'scale(1) translateY(0)' : 'scale(0.96) translateY(8px)',
        }}
      >
        {/* Header */}
        <div className="flex flex-col items-center gap-4 p-5">
          <div className={cn('flex items-center justify-center rounded-[10px] p-2.5', p.bg)}>
            <span className={cn('flex size-5 items-center justify-center', p.icon)}>
              {icon ?? statusIcon[status]}
            </span>
          </div>
          <div className="flex flex-col items-center gap-1 text-center">
            <h2 className="text-label-md text-text-strong-950">{title}</h2>
            {description && (
              <p className="text-paragraph-sm text-text-sub-600">{description}</p>
            )}
          </div>
        </div>

        {/* Footer */}
        <div className="flex items-center gap-3 border-t border-stroke-soft-200 px-5 py-4">
          {showDontShowAgain && (
            <Checkbox
              label={dontShowAgainLabel}
              checked={dontShowAgain}
              onChange={(e) => onDontShowAgainChange?.(e.target.checked)}
              wrapperClassName="shrink-0"
            />
          )}
          <div className="flex flex-1 items-center justify-end gap-3">
            {!hideCancel && (
              <Button size="small" mode="stroke" variant="neutral" onClick={onCancel ?? onClose}>
                {cancelLabel}
              </Button>
            )}
            <Button size="small" variant={resolvedConfirmVariant} isLoading={isLoading} onClick={onConfirm}>
              {confirmLabel}
            </Button>
          </div>
        </div>
      </div>
    </div>,
    target,
  )
}
