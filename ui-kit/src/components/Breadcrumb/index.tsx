import { Fragment } from 'react'
import { cn } from '@/lib/cn'
import { ChevronRightIcon } from '@/lib/icons'
import { Link } from '@/lib/link'

export type BreadcrumbItem = {
  label: string
  href?: string
  icon?: React.ReactNode
  onClick?: () => void
}

export type BreadcrumbProps = {
  items: BreadcrumbItem[]
  separator?: React.ReactNode
  className?: string
}

export default function Breadcrumb({ items, separator, className }: BreadcrumbProps) {
  return (
    <nav aria-label="Breadcrumb" className={className}>
      <ol className="flex items-center gap-1.5">
        {items.map((item, i) => {
          const isLast = i === items.length - 1
          const content = (
            <span
              className={cn(
                'inline-flex items-center gap-1 text-label-sm transition-colors',
                isLast ? 'text-text-strong-950' : 'text-text-sub-600 hover:text-text-strong-950',
              )}
            >
              {item.icon && <span className="flex size-5 items-center justify-center">{item.icon}</span>}
              {item.label}
            </span>
          )
          return (
            <Fragment key={i}>
              <li>
                {isLast || (!item.href && !item.onClick) ? (
                  content
                ) : item.href ? (
                  <Link href={item.href}>{content}</Link>
                ) : (
                  <button type="button" onClick={item.onClick}>
                    {content}
                  </button>
                )}
              </li>
              {!isLast && (
                <li aria-hidden className="flex items-center text-text-soft-400">
                  {separator ?? <ChevronRightIcon className="size-4" />}
                </li>
              )}
            </Fragment>
          )
        })}
      </ol>
    </nav>
  )
}
