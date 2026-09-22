import { useEffect, useState } from 'react'
import { createPortal } from 'react-dom'
import { cn } from '@/lib/cn'
import Button from '@/components/Button'
import type { ButtonVariant } from '@/components/Button'
import { CloseIcon } from '@/lib/icons'

export type DialogModalProps = {
  open: boolean
  onClose: () => void
  /** Optional circular, bordered header icon. */
  icon?: React.ReactNode
  title: React.ReactNode
  subtitle?: React.ReactNode
  /** Body content. */
  children?: React.ReactNode

  /** Custom footer. When omitted, the default Cancel + confirm actions render. */
  footer?: React.ReactNode
  hideFooter?: boolean

  /** Default footer actions (used only when `footer` is not provided). */
  confirmLabel?: string
  onConfirm?: () => void
  confirmVariant?: ButtonVariant
  isLoading?: boolean
  isDisabled?: boolean
  cancelLabel?: string
  onCancel?: () => void
  hideCancel?: boolean

  showClose?: boolean
  maxWidthClassName?: string
  bodyClassName?: string
  className?: string
}

export default function DialogModal({
  open,
  onClose,
  icon,
  title,
  subtitle,
  children,
  footer,
  hideFooter = false,
  confirmLabel = 'Apply Changes',
  onConfirm,
  confirmVariant = 'primary',
  isLoading = false,
  isDisabled = false,
  cancelLabel = 'Cancel',
  onCancel,
  hideCancel = false,
  showClose = true,
  maxWidthClassName = 'w-[480px] max-w-[calc(100vw-2rem)]',
  bodyClassName,
  className,
}: DialogModalProps) {
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

  const target = document.getElementById('portal-root') ?? document.body
  const showDefaultFooter = !hideFooter && (footer != null || onConfirm != null || !hideCancel)

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
        role="dialog"
        aria-modal="true"
        onClick={(e) => e.stopPropagation()}
        className={cn(
          'flex max-h-[calc(100vh-2rem)] flex-col overflow-hidden rounded-[20px] border border-stroke-soft-200 bg-bg-white-0 shadow-[var(--shadow-md)] transition-all duration-200',
          maxWidthClassName,
          className,
        )}
        style={{
          opacity: visible ? 1 : 0,
          transform: visible ? 'scale(1) translateY(0)' : 'scale(0.96) translateY(8px)',
        }}
      >
        {/* Header */}
        <div className="flex items-start gap-4 border-b border-stroke-soft-200 p-4">
          {icon && (
            <span className="flex size-10 shrink-0 items-center justify-center rounded-full border border-stroke-soft-200 text-text-sub-600">
              <span className="flex size-5 items-center justify-center">{icon}</span>
            </span>
          )}
          <div className="flex min-w-0 flex-1 flex-col gap-1">
            <h2 className="text-label-sm text-text-strong-950">{title}</h2>
            {subtitle && <p className="text-paragraph-xs text-text-sub-600">{subtitle}</p>}
          </div>
          {showClose && (
            <button
              type="button"
              onClick={onClose}
              aria-label="Close"
              className="flex size-6 shrink-0 items-center justify-center rounded-md text-text-sub-600 shadow-[var(--shadow-xs)] transition-colors hover:bg-bg-weak-50 hover:text-text-strong-950"
            >
              <CloseIcon className="size-5" />
            </button>
          )}
        </div>

        {/* Body */}
        {children != null && (
          <div className={cn('overflow-auto px-5', bodyClassName)}>{children}</div>
        )}

        {/* Footer */}
        {showDefaultFooter &&
          (footer != null ? (
            <div className="flex items-center gap-3 border-t border-stroke-soft-200 px-5 py-4">
              {footer}
            </div>
          ) : (
            <div className="flex items-center border-t border-stroke-soft-200 px-5 py-4">
              <div className="flex flex-1 items-center justify-end gap-3">
                {!hideCancel && (
                  <Button size="small" mode="stroke" variant="neutral" onClick={onCancel ?? onClose}>
                    {cancelLabel}
                  </Button>
                )}
                {onConfirm && (
                  <Button size="small" disabled={isDisabled} variant={confirmVariant} isLoading={isLoading} onClick={onConfirm}>
                    {confirmLabel}
                  </Button>
                )}
              </div>
            </div>
          ))}
      </div>
    </div>,
    target,
  )
}
