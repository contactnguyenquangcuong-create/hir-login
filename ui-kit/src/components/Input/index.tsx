import { forwardRef, useId } from 'react'
import { cn } from '@/lib/cn'
import { AlertCircleIcon } from '@/lib/icons'

export type InputSize = 'medium' | 'small'

export type InputProps = Omit<React.InputHTMLAttributes<HTMLInputElement>, 'size'> & {
  label?: string
  hint?: string
  error?: string
  inputSize?: InputSize
  leftIcon?: React.ReactNode
  rightIcon?: React.ReactNode
  /** Static text glued to the left (e.g. "https://"). */
  leftAddon?: React.ReactNode
  /** Static text glued to the right (e.g. ".com"). */
  rightAddon?: React.ReactNode
  wrapperClassName?: string
}

const Input = forwardRef<HTMLInputElement, InputProps>(function Input(
  {
    label,
    hint,
    error,
    inputSize = 'medium',
    leftIcon,
    rightIcon,
    leftAddon,
    rightAddon,
    className,
    wrapperClassName,
    disabled,
    id,
    ...rest
  },
  ref,
) {
  const autoId = useId()
  const inputId = id ?? autoId
  const hasError = Boolean(error)

  return (
    <div className={cn('flex w-full flex-col gap-1', wrapperClassName)}>
      {label && (
        <label htmlFor={inputId} className="text-label-sm text-text-strong-950">
          {label}
        </label>
      )}
      <div
        className={cn(
          'flex items-center gap-2 overflow-hidden rounded-[10px] bg-bg-white-0 text-text-strong-950 transition-shadow',
          'ring-1 ring-inset ring-stroke-soft-200',
          inputSize === 'medium' ? 'h-10 px-3' : 'h-9 px-2.5',
          !disabled && !hasError && 'focus-within:ring-2 focus-within:ring-primary-base focus-within:shadow-[var(--ring-primary-focus)]',
          hasError && 'ring-error-base focus-within:shadow-[var(--ring-error-focus)]',
          disabled && 'pointer-events-none bg-bg-weak-50 text-text-disabled-300 ring-stroke-soft-200',
          className,
        )}
      >
        {leftAddon && <span className="text-paragraph-sm text-text-soft-400">{leftAddon}</span>}
        {leftIcon && <span className="flex size-5 shrink-0 items-center justify-center text-text-soft-400">{leftIcon}</span>}
        <input
          ref={ref}
          id={inputId}
          disabled={disabled}
          // 16px on small screens: anything below 16px makes iOS Safari zoom the page on focus
          className="h-full w-full min-w-0 bg-transparent text-paragraph-sm max-sm:text-[16px] text-text-strong-950 outline-none placeholder:text-text-soft-400 disabled:cursor-not-allowed"
          aria-invalid={hasError || undefined}
          {...rest}
        />
        {rightIcon && !hasError && <span className="flex size-5 shrink-0 items-center justify-center text-text-soft-400">{rightIcon}</span>}
        {hasError && <AlertCircleIcon className="size-5 shrink-0 text-error-base" />}
        {rightAddon && <span className="text-paragraph-sm text-text-soft-400">{rightAddon}</span>}
      </div>
      {(hint || error) && (
        <p className={cn('text-paragraph-xs', hasError ? 'text-error-base' : 'text-text-sub-600')}>
          {error ?? hint}
        </p>
      )}
    </div>
  )
})

export default Input
