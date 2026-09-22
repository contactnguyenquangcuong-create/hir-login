/**
 * Shardx UI Kit — public entry point.
 *
 * Consume from another project:
 *   import { Button, ThemeProvider } from 'shardx-ui-kit'
 *   import 'shardx-ui-kit/styles.css'
 */
export * from './components'
export * as Icons from './icons'
export { ThemeProvider, useTheme } from './theme/ThemeProvider'
export { default as ThemeToggle } from './theme/ThemeToggle'
export type { Theme, ResolvedTheme } from './theme/ThemeProvider'
export { LinkProvider, Link, useLinkComponent } from './lib/link'
export type { LinkComponent, LinkComponentProps } from './lib/link'
export { cn } from './lib/cn'
