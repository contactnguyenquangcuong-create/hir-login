import { useEffect, useState } from 'react'
import { createPortal } from 'react-dom'
import { cn } from '@/lib/cn'
import { CloseIcon } from '@/lib/icons'

export type ModalProps = {
  open: boolean
  onClose: () => void
  title?: React.ReactNode
  description?: React.ReactNode
  children?: React.ReactNode
  footer?: React.ReactNode
  /** Max width utility class, e.g. 'max-w-md'. */
  maxWidthClassName?: string
  showClose?: boolean
}

export default function Modal({
  open,
  onClose,
  title,
  description,
  children,
  footer,
  maxWidthClassName = 'max-w-lg',
  showClose = true,
}: ModalProps) {
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
          'relative flex w-full flex-col rounded-2xl bg-bg-white-0 shadow-[var(--shadow-md)] transition-all duration-200',
          maxWidthClassName,
        )}
        style={{
          opacity: visible ? 1 : 0,
          transform: visible ? 'scale(1) translateY(0)' : 'scale(0.96) translateY(8px)',
        }}
      >
        {(title || showClose) && (
          <div className="flex items-start justify-between gap-4 p-5 pb-0">
            <div className="flex flex-col gap-1">
              {title && <h2 className="text-label-lg text-text-strong-950">{title}</h2>}
              {description && <p className="text-paragraph-sm text-text-sub-600">{description}</p>}
            </div>
            {showClose && (
              <button
                type="button"
                onClick={onClose}
                aria-label="Close"
                className="-mr-1 -mt-1 flex size-8 shrink-0 items-center justify-center rounded-lg text-text-soft-400 transition-colors hover:bg-bg-weak-50 hover:text-text-strong-950"
              >
                <CloseIcon className="size-5" />
              </button>
            )}
          </div>
        )}
        {children && <div className="p-5 text-paragraph-sm text-text-sub-600">{children}</div>}
        {footer && <div className="flex justify-end gap-3 border-t border-stroke-soft-200 p-4">{footer}</div>}
      </div>
    </div>,
    target,
  )
}
