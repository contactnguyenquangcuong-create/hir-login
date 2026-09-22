import { forwardRef, useId } from 'react'
import { cn } from '@/lib/cn'

export type SwitchProps = Omit<
  React.InputHTMLAttributes<HTMLInputElement>,
  'type' | 'size' | 'onChange'
> & {
  label?: React.ReactNode
  onChange?: (checked: boolean) => void
  wrapperClassName?: string
}

const Switch = forwardRef<HTMLInputElement, SwitchProps>(function Switch(
  { label, onChange, className, wrapperClassName, disabled, id, ...rest },
  ref,
) {
  const autoId = useId()
  const switchId = id ?? autoId

  return (
    <label
      htmlFor={switchId}
      className={cn(
        'inline-flex cursor-pointer items-center gap-2',
        disabled && 'cursor-not-allowed opacity-60',
        wrapperClassName,
      )}
    >
      <span className="relative inline-flex">
        <input
          ref={ref}
          id={switchId}
          type="checkbox"
          disabled={disabled}
          onChange={(e) => onChange?.(e.target.checked)}
          className={cn(
            'peer h-5 w-8 shrink-0 cursor-pointer appearance-none rounded-full bg-bg-sub-300 transition-colors',
            'checked:bg-primary-base',
            'focus-visible:outline-none focus-visible:shadow-[var(--ring-primary-focus)]',
            disabled && 'cursor-not-allowed',
            className,
          )}
          {...rest}
        />
        <span className="pointer-events-none absolute left-0.5 top-0.5 size-4 rounded-full bg-static-white shadow-[var(--shadow-sm)] transition-transform peer-checked:translate-x-3" />
      </span>
      {label && <span className="text-paragraph-sm text-text-strong-950">{label}</span>}
    </label>
  )
})

export default Switch
