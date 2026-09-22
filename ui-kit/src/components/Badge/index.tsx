import { HugeiconsIcon } from '@hugeicons/react'
import type { IconSvgElement } from '@hugeicons/react'
import { CheckmarkCircle02Icon, Alert02Icon, CancelCircleIcon } from '@hugeicons/core-free-icons'
import { cn } from '@/lib/cn'

export type BadgeStatus =
  | 'success'
  | 'pending'
  | 'error'
  | 'paid'
  | 'unpaid'
  | 'active'
  | 'on-hold'
  | 'cancelled'
  | 'refunded'

export type BadgeProps = React.HTMLAttributes<HTMLSpanElement> & {
  /** Известные статусы типизированы; любая другая строка рендерится
   * нейтральным бейджем (фолбэк), а не роняет SSR. */
  status: BadgeStatus | (string & {})
}

// Hugeicons glyph per status (success ✓, warning !, failed ✕).
const statusToIcon: Record<BadgeStatus, IconSvgElement> = {
  success: CheckmarkCircle02Icon,
  pending: Alert02Icon,
  error: CancelCircleIcon,
  paid: CheckmarkCircle02Icon,
  unpaid: CancelCircleIcon,
  active: CheckmarkCircle02Icon,
  'on-hold': Alert02Icon,
  cancelled: CancelCircleIcon,
  refunded: CancelCircleIcon,
}

// Text + soft-background pair per status (kit state tokens; on-hold = neutral).

const statusToColor = {
  success: "text-success bg-success-background",
  pending: "text-warning bg-warning-background",
  error: "text-error bg-error-background",
  paid: "text-success bg-success-background",
  unpaid: "text-error bg-error-background",
  active: "text-success bg-success-background",
  "on-hold": "dark:text-[var(--color-default)] dark:bg-[var(--color-default-background)] text-[var(--color-default-light)] bg-[var(--color-default-light-background)]",
  cancelled: "text-error bg-error-background",
  refunded: "dark:text-[var(--color-default)] dark:bg-[var(--color-default-background)] text-[var(--color-default-light)] bg-[var(--color-default-light-background)]",
}

// Неизвестный статус (новое значение из данных) не должен ронять рендер:
// нейтральный вид + warning-глиф.
const fallbackIcon = Alert02Icon
const fallbackColor =
  "dark:text-[var(--color-default)] dark:bg-[var(--color-default-background)] text-[var(--color-default-light)] bg-[var(--color-default-light-background)]"


export default function Badge({ status, className, ...rest }: BadgeProps) {
  return (
    <span
      className={cn(
        'inline-flex min-w-24 items-center justify-center gap-1 rounded-lg py-1 pl-1 pr-2 text-center text-label-xs font-medium',
        statusToColor[status as BadgeStatus] ?? fallbackColor,
        className,
      )}
      {...rest}
    >
      <HugeiconsIcon icon={statusToIcon[status as BadgeStatus] ?? fallbackIcon} size={16} className="size-4 shrink-0" />
      <span>{status.charAt(0).toUpperCase() + status.slice(1)}</span>
    </span>
  )
}
