import { cn } from '@/lib/cn'
import { Link } from '@/lib/link'

export type TabItem = {
  label: string
  value: string
  icon?: React.ReactNode
  disabled?: boolean
  url?: string
}

export type TabsProps = {
  items: TabItem[]
  value: string
  /** Underline (default) or pill-style segmented tabs. */
  variant?: 'line' | 'pill'
  className?: string
}

export default function Tabs({ items, value, variant = 'line', className }: TabsProps) {
  if (variant === 'pill') {
    return (
      <div className={cn('inline-flex items-center gap-1 rounded-[10px] bg-bg-weak-50 p-1', className)}>
        {items.map((item) => {
          const active = item.value === value
          return (
            <Link
              key={item.value}
              href={item.url ?? '#'}
              className={cn(
                'inline-flex h-8 items-center gap-1.5 rounded-md px-3 text-label-sm transition-colors disabled:opacity-50',
                active
                  ? 'bg-bg-white-0 text-text-strong-950 shadow-[var(--shadow-xs)]'
                  : 'text-text-sub-600 hover:text-text-strong-950',
              )}
            >
              {item.icon && <span className="flex size-5 items-center justify-center">{item.icon}</span>}
              {item.label}
            </Link>
          )
        })}
      </div>
    )
  }

  return (
    <div className={cn('flex items-center gap-6 border-b border-stroke-soft-200', className)}>
      {items.map((item) => {
        const active = item.value === value
        return (
          <Link
            key={item.value}
            href={item.url ?? '#'}
            className={cn(
              'relative inline-flex items-center gap-1.5 py-3 text-label-sm transition-colors disabled:opacity-50',
              active ? 'text-text-strong-950' : 'text-text-sub-600 hover:text-text-strong-950',
            )}
          >
            {item.icon && (
              <span className={cn('flex size-5 items-center justify-center', active ? 'text-primary-base' : 'text-text-soft-400')}>
                {item.icon}
              </span>
            )}
            {item.label}
            {active && <span className="absolute inset-x-0 -bottom-px h-0.5 rounded-full bg-primary-base" />}
          </Link>
        )
      })}
    </div>
  )
}
