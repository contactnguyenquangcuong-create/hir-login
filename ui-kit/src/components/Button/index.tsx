import { forwardRef } from 'react'
import { cn } from '@/lib/cn'
import { SpinnerIcon } from '@/lib/icons'

export type ButtonVariant = 'primary' | 'neutral' | 'error'
export type ButtonMode = 'filled' | 'stroke' | 'lighter' | 'ghost'
export type ButtonSize = 'medium' | 'small' | 'xsmall' | '2xsmall'

export type ButtonProps = Omit<
  React.ButtonHTMLAttributes<HTMLButtonElement>,
  'children'
> & {
  variant?: ButtonVariant
  mode?: ButtonMode
  size?: ButtonSize
  fullRadius?: boolean
  onlyIcon?: boolean
  isLoading?: boolean
  leftIcon?: React.ReactNode
  rightIcon?: React.ReactNode
  children?: React.ReactNode
}

const sizes: Record<ButtonSize, { base: string; radius: string; onlyIcon: string }> = {
  medium: { base: 'h-10 gap-1 px-2.5', radius: 'rounded-[10px]', onlyIcon: 'w-10 px-0' },
  small: { base: 'h-9 gap-1 px-2', radius: 'rounded-lg', onlyIcon: 'w-9 px-0' },
  xsmall: { base: 'h-8 gap-1 px-1.5', radius: 'rounded-lg', onlyIcon: 'w-8 px-0' },
  '2xsmall': { base: 'h-7 gap-1 px-1', radius: 'rounded-md', onlyIcon: 'w-7 px-0' },
}

const looks: Record<ButtonMode, Record<ButtonVariant, string>> = {
  filled: {
    primary: 'bg-primary-base text-static-white hover:bg-primary-darker',
    neutral: 'bg-[var(--btn-neutral-bg)] text-[var(--btn-neutral-fg)] hover:bg-[var(--btn-neutral-bg-hover)]',
    error: 'bg-error-base text-static-white hover:bg-red-600',
  },
  stroke: {
    primary: 'bg-bg-white-0 text-primary-base ring-1 ring-inset ring-primary-base hover:bg-primary-alpha-10',
    neutral: 'bg-bg-white-0 text-text-sub-600 ring-1 ring-inset ring-stroke-soft-200 shadow-[var(--shadow-xs)] hover:bg-bg-weak-50',
    error: 'bg-bg-white-0 text-error-base ring-1 ring-inset ring-error-base hover:bg-error-weak',
  },
  lighter: {
    primary: 'bg-primary-alpha-10 text-primary-base hover:bg-primary-alpha-16',
    neutral: 'bg-bg-weak-50 text-text-sub-600 hover:bg-bg-soft-200',
    error: 'bg-error-weak text-error-base hover:bg-error-soft',
  },
  ghost: {
    primary: 'bg-transparent text-primary-base hover:bg-primary-alpha-10',
    neutral: 'bg-transparent text-text-sub-600 hover:bg-bg-weak-50',
    error: 'bg-transparent text-error-base hover:bg-error-weak',
  },
}

const focusRing: Record<ButtonVariant, string> = {
  primary: 'focus-visible:shadow-[var(--ring-primary-focus)]',
  neutral: 'focus-visible:shadow-[var(--ring-neutral-focus)]',
  error: 'focus-visible:shadow-[var(--ring-error-focus)]',
}

const Button = forwardRef<HTMLButtonElement, ButtonProps>(function Button(
  {
    variant = 'primary',
    mode = 'filled',
    size = 'medium',
    fullRadius = false,
    onlyIcon = false,
    isLoading = false,
    leftIcon,
    rightIcon,
    children,
    className,
    disabled,
    type = 'button',
    ...rest
  },
  ref,
) {
  const s = sizes[size]
  const isDisabled = disabled || isLoading

  return (
    <button
      ref={ref}
      type={type}
      disabled={isDisabled}
      className={cn(
        'relative inline-flex shrink-0 select-none items-center justify-center text-label-sm outline-none transition duration-200 ease-out',
        s.base,
        fullRadius ? 'rounded-full' : s.radius,
        onlyIcon && s.onlyIcon,
        isDisabled
          ? 'pointer-events-none bg-bg-weak-50 text-text-disabled-300'
          : cn(looks[mode][variant], focusRing[variant]),
        className,
      )}
      {...rest}
    >
      {isLoading ? (
        <SpinnerIcon className="size-5 animate-spin" />
      ) : (
        <>
          {leftIcon && <span className="flex size-5 shrink-0 items-center justify-center">{leftIcon}</span>}
          {!onlyIcon && children != null && <span className="px-1">{children}</span>}
          {rightIcon && <span className="flex size-5 shrink-0 items-center justify-center">{rightIcon}</span>}
        </>
      )}
    </button>
  )
})

export default Button
