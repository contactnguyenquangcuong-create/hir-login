import { forwardRef, useId } from 'react'
import { cn } from '@/lib/cn'

export type TextareaProps = React.TextareaHTMLAttributes<HTMLTextAreaElement> & {
  label?: string
  hint?: string
  error?: string
  /** Show a character counter (requires `maxLength`). */
  showCount?: boolean
  wrapperClassName?: string
}

const Textarea = forwardRef<HTMLTextAreaElement, TextareaProps>(function Textarea(
  { label, hint, error, showCount, className, wrapperClassName, disabled, id, maxLength, value, defaultValue, ...rest },
  ref,
) {
  const autoId = useId()
  const areaId = id ?? autoId
  const hasError = Boolean(error)
  const length = typeof value === 'string' ? value.length : typeof defaultValue === 'string' ? defaultValue.length : 0

  return (
    <div className={cn('flex w-full flex-col gap-1', wrapperClassName)}>
      {label && (
        <label htmlFor={areaId} className="text-label-sm text-text-strong-950">
          {label}
        </label>
      )}
      <div
        className={cn(
          'rounded-xl bg-bg-white-0 p-3 ring-1 ring-inset ring-stroke-soft-200 transition-shadow',
          !disabled && !hasError && 'focus-within:ring-2 focus-within:ring-primary-base focus-within:shadow-[var(--ring-primary-focus)]',
          hasError && 'ring-error-base focus-within:shadow-[var(--ring-error-focus)]',
          disabled && 'pointer-events-none bg-bg-weak-50',
          className,
        )}
      >
        <textarea
          ref={ref}
          id={areaId}
          disabled={disabled}
          maxLength={maxLength}
          value={value}
          defaultValue={defaultValue}
          aria-invalid={hasError || undefined}
          className="min-h-20 w-full resize-y bg-transparent text-paragraph-sm text-text-strong-950 outline-none placeholder:text-text-soft-400 disabled:cursor-not-allowed disabled:text-text-disabled-300"
          {...rest}
        />
        {showCount && maxLength != null && (
          <div className="mt-1 text-right text-paragraph-xs text-text-soft-400">
            {length}/{maxLength}
          </div>
        )}
      </div>
      {(hint || error) && (
        <p className={cn('text-paragraph-xs', hasError ? 'text-error-base' : 'text-text-sub-600')}>
          {error ?? hint}
        </p>
      )}
    </div>
  )
})

export default Textarea
