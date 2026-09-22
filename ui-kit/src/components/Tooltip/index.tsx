import { useState } from 'react'
import { cn } from '@/lib/cn'

export type TooltipSide = 'top' | 'bottom' | 'left' | 'right'

export type TooltipProps = {
  content: React.ReactNode
  side?: TooltipSide
  children: React.ReactNode
  className?: string
}

const sideClasses: Record<TooltipSide, string> = {
  top: 'bottom-full left-1/2 mb-2 -translate-x-1/2',
  bottom: 'top-full left-1/2 mt-2 -translate-x-1/2',
  left: 'right-full top-1/2 mr-2 -translate-y-1/2',
  right: 'left-full top-1/2 ml-2 -translate-y-1/2',
}

export default function Tooltip({ content, side = 'top', children, className }: TooltipProps) {
  const [open, setOpen] = useState(false)

  return (
    <span
      className="relative inline-flex"
      onMouseEnter={() => setOpen(true)}
      onMouseLeave={() => setOpen(false)}
      onFocus={() => setOpen(true)}
      onBlur={() => setOpen(false)}
    >
      {children}
      {open && (
        <span
          role="tooltip"
          className={cn(
            'pointer-events-none absolute z-50 w-max max-w-xs rounded-lg bg-bg-strong-950 px-2.5 py-1.5 text-label-xs text-text-white-0 shadow-[var(--shadow-tooltip)]',
            sideClasses[side],
            className,
          )}
        >
          {content}
        </span>
      )}
    </span>
  )
}
