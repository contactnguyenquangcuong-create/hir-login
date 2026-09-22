import { forwardRef, useId } from 'react'
import { cn } from '@/lib/cn'
import { CheckIcon, MinusIcon } from '@/lib/icons'

export type CheckboxProps = Omit<
  React.InputHTMLAttributes<HTMLInputElement>,
  'type' | 'size'
> & {
  label?: React.ReactNode
  /** Renders the indeterminate (dash) state. */
  indeterminate?: boolean
  wrapperClassName?: string
}

const Checkbox = forwardRef<HTMLInputElement, CheckboxProps>(function Checkbox(
  { label, indeterminate = false, className, wrapperClassName, disabled, checked, id, ...rest },
  ref,
) {
  const autoId = useId()
  const boxId = id ?? autoId
  const isOn = Boolean(checked) || indeterminate

  return (
    <label
      htmlFor={boxId}
      className={cn(
        'inline-flex cursor-pointer items-center gap-2',
        disabled && 'cursor-not-allowed opacity-60',
        wrapperClassName,
      )}
    >
      <span className="relative flex size-5 items-center justify-center">
        <input
          ref={ref}
          id={boxId}
          type="checkbox"
          checked={checked}
          disabled={disabled}
          className={cn(
            'peer size-5 shrink-0 appearance-none rounded-md bg-bg-white-0 ring-1 ring-inset ring-stroke-sub-300 transition',
            'hover:ring-stroke-strong-950 checked:bg-primary-base checked:ring-0 checked:hover:bg-primary-darker',
            indeterminate && 'bg-primary-base ring-0 hover:bg-primary-darker',
            'focus-visible:outline-none focus-visible:shadow-[var(--ring-primary-focus)]',
            className,
          )}
          {...rest}
        />
        <span
          className={cn(
            'pointer-events-none absolute inset-0 flex items-center justify-center text-static-white transition-opacity',
            isOn ? 'opacity-100' : 'opacity-0',
          )}
        >
          {indeterminate ? <MinusIcon className="size-3.5" /> : <CheckIcon className="size-3.5" />}
        </span>
      </span>
      {label && <span className="text-paragraph-sm text-text-strong-950">{label}</span>}
    </label>
  )
})

export default Checkbox
