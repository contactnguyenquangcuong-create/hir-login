import { cn } from '@/lib/cn'
import { CloseIcon } from '@/lib/icons'

export type TagVariant = 'stroke' | 'gray'

export type TagProps = React.HTMLAttributes<HTMLSpanElement> & {
  variant?: TagVariant
  leftIcon?: React.ReactNode
  /** Renders a trailing remove button and calls this on click. */
  onRemove?: () => void
  disabled?: boolean
  children?: React.ReactNode
}

export default function Tag({
  variant = 'stroke',
  leftIcon,
  onRemove,
  disabled = false,
  children,
  className,
  ...rest
}: TagProps) {
  return (
    <span
      className={cn(
        'inline-flex h-6 items-center gap-1.5 rounded-md pl-2 pr-1.5 text-label-xs',
        variant === 'stroke'
          ? 'bg-bg-white-0 text-text-sub-600 ring-1 ring-inset ring-stroke-soft-200'
          : 'bg-bg-weak-50 text-text-sub-600',
        disabled && 'opacity-50',
        !onRemove && 'pr-2',
        className,
      )}
      {...rest}
    >
      {leftIcon && <span className="flex size-3.5 shrink-0 items-center justify-center text-text-soft-400">{leftIcon}</span>}
      {children}
      {onRemove && (
        <button
          type="button"
          onClick={onRemove}
          disabled={disabled}
          aria-label="Remove"
          className="flex size-4 items-center justify-center rounded text-text-soft-400 transition-colors hover:text-text-strong-950"
        >
          <CloseIcon className="size-3.5" />
        </button>
      )}
    </span>
  )
}
