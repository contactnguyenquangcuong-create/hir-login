import { cn } from '@/lib/cn'
import {
  AlertCircleIcon,
  AlertTriangleIcon,
  CheckCircleIcon,
  CloseIcon,
  InfoIcon,
} from '@/lib/icons'

export type AlertStatus = 'information' | 'success' | 'warning' | 'error' | 'feature'
export type AlertVariant = 'filled' | 'light' | 'lighter' | 'stroke'

export type AlertProps = {
  status?: AlertStatus
  variant?: AlertVariant
  title?: React.ReactNode
  children?: React.ReactNode
  icon?: React.ReactNode
  onClose?: () => void
  className?: string
}

const statusIcon: Record<AlertStatus, React.ReactNode> = {
  information: <InfoIcon className="size-5" />,
  success: <CheckCircleIcon className="size-5" />,
  warning: <AlertTriangleIcon className="size-5" />,
  error: <AlertCircleIcon className="size-5" />,
  feature: <InfoIcon className="size-5" />,
}

// [status] -> { base bg, soft bg, weak bg, weak bg in dark (more pronounced), accent text }
const palette: Record<
  AlertStatus,
  { base: string; soft: string; weak: string; weakDark: string; ring: string; accent: string }
> = {
  information: { base: 'bg-information-base', soft: 'bg-information-soft', weak: 'bg-information-weak', weakDark: 'dark:bg-information-soft', ring: 'ring-information-base', accent: 'text-information-base' },
  success: { base: 'bg-success-base', soft: 'bg-success-soft', weak: 'bg-success-weak', weakDark: 'dark:bg-success-soft', ring: 'ring-success-base', accent: 'text-success-base' },
  warning: { base: 'bg-warning-base', soft: 'bg-warning-soft', weak: 'bg-warning-weak', weakDark: 'dark:bg-warning-soft', ring: 'ring-warning-base', accent: 'text-warning-base' },
  error: { base: 'bg-error-base', soft: 'bg-error-soft', weak: 'bg-error-weak', weakDark: 'dark:bg-error-soft', ring: 'ring-error-base', accent: 'text-error-base' },
  feature: { base: 'bg-primary-base', soft: 'bg-purple-200', weak: 'bg-purple-50', weakDark: 'dark:bg-purple-200', ring: 'ring-primary-base', accent: 'text-primary-base' },
}

export default function Alert({
  status = 'information',
  variant = 'lighter',
  title,
  children,
  icon,
  onClose,
  className,
}: AlertProps) {
  const p = palette[status]

  // Variants that sit on a light/pastel background in BOTH themes. Their text
  // must stay dark in dark mode (otherwise white text lands on a pale bg).
  const isTinted = variant === 'light' || variant === 'lighter'

  const container =
    variant === 'filled'
      ? cn(p.base, 'text-static-white')
      : variant === 'light'
        ? cn(p.soft, 'text-text-strong-950 dark:text-static-black')
        : variant === 'stroke'
          ? cn('bg-bg-white-0 text-text-strong-950 ring-1 ring-inset ring-stroke-soft-200 shadow-[var(--shadow-xs)]')
          : cn(p.weak, p.weakDark, 'text-text-strong-950 dark:text-static-black')

  const iconColor = variant === 'filled' ? 'text-static-white' : p.accent
  const bodyColor = variant === 'filled'
    ? 'text-static-white/90'
    : isTinted
      ? 'text-text-sub-600 dark:text-neutral-700'
      : 'text-text-sub-600'

  return (
    <div className={cn('flex w-full gap-3 rounded-xl p-4', container, className)} role="alert">
      <span className={cn('mt-0.5 flex size-5 shrink-0 items-center justify-center', iconColor)}>
        {icon ?? statusIcon[status]}
      </span>
      <div className="flex min-w-0 flex-1 flex-col gap-1">
        {title && <div className="text-label-sm">{title}</div>}
        {children && <div className={cn('text-paragraph-sm', bodyColor)}>{children}</div>}
      </div>
      {onClose && (
        <button
          type="button"
          onClick={onClose}
          aria-label="Dismiss"
          className={cn(
            'flex size-5 shrink-0 items-center justify-center rounded transition-opacity',
            variant === 'filled'
              ? 'text-static-white/80 hover:text-static-white'
              : isTinted
                ? 'text-text-soft-400 hover:text-text-strong-950 dark:text-neutral-500 dark:hover:text-static-black'
                : 'text-text-soft-400 hover:text-text-strong-950',
          )}
        >
          <CloseIcon className="size-5" />
        </button>
      )}
    </div>
  )
}
