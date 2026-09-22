import { cn } from '@/lib/cn'

export type ProgressColor = 'primary' | 'success' | 'error' | 'warning' | 'gray'

export type ProgressBarProps = {
  value: number
  max?: number
  color?: ProgressColor
  /** Show the numeric percentage on the right. */
  showValue?: boolean
  className?: string
}

const fillColor: Record<ProgressColor, string> = {
  primary: 'bg-primary-base',
  success: 'bg-success-base',
  error: 'bg-error-base',
  warning: 'bg-warning-base',
  gray: 'bg-neutral-400',
}

export default function ProgressBar({
  value,
  max = 100,
  color = 'primary',
  showValue = false,
  className,
}: ProgressBarProps) {
  const pct = Math.min(100, Math.max(0, (value / max) * 100))

  return (
    <div className={cn('flex items-center gap-3', className)}>
      <div
        className="h-1.5 w-full overflow-hidden rounded-full bg-bg-soft-200"
        role="progressbar"
        aria-valuenow={Math.round(pct)}
        aria-valuemin={0}
        aria-valuemax={100}
      >
        <div
          className={cn('h-full rounded-full transition-all duration-300', fillColor[color])}
          style={{ width: `${pct}%` }}
        />
      </div>
      {showValue && (
        <span className="min-w-9 shrink-0 text-right text-label-sm tabular-nums text-text-sub-600">
          {Math.round(pct)}%
        </span>
      )}
    </div>
  )
}
