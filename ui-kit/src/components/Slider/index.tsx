import { useId } from 'react'
import { cn } from '@/lib/cn'

export type SliderProps = {
  value: number
  min?: number
  max?: number
  step?: number
  onChange?: (value: number) => void
  label?: string
  /** Text shown at the right of the label row (e.g. the value). */
  valueLabel?: React.ReactNode
  disabled?: boolean
  className?: string
}

export default function Slider({
  value,
  min = 0,
  max = 100,
  step = 1,
  onChange,
  label,
  valueLabel,
  disabled = false,
  className,
}: SliderProps) {
  const id = useId()
  const pct = ((value - min) / (max - min)) * 100

  return (
    <div className={cn('flex w-full flex-col gap-2', className)}>
      {(label || valueLabel != null) && (
        <div className="flex items-center justify-between">
          {label && (
            <label htmlFor={id} className="text-label-sm text-text-strong-950">
              {label}
            </label>
          )}
          {valueLabel != null && <span className="text-label-sm text-text-sub-600">{valueLabel}</span>}
        </div>
      )}
      <input
        id={id}
        type="range"
        min={min}
        max={max}
        step={step}
        value={value}
        disabled={disabled}
        onChange={(e) => onChange?.(Number(e.target.value))}
        className={cn(
          'sx-slider h-1.5 w-full cursor-pointer appearance-none rounded-full outline-none',
          disabled && 'cursor-not-allowed opacity-60',
        )}
        style={{
          background: `linear-gradient(to right, var(--color-primary-base) 0%, var(--color-primary-base) ${pct}%, var(--bg-soft-200) ${pct}%, var(--bg-soft-200) 100%)`,
        }}
      />
    </div>
  )
}
