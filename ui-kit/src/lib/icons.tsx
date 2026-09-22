/**
 * Kit icon set — thin wrappers over @hugeicons/core-free-icons rendered via
 * <HugeiconsIcon>. Export names and the (SVGProps) signature are unchanged, so
 * every component keeps importing the same names; size/colour still come from
 * the parent via `className` (glyphs use currentColor).
 */
import { HugeiconsIcon } from '@hugeicons/react'
import type { IconSvgElement } from '@hugeicons/react'
import {
  Tick02Icon,
  MinusSignIcon,
  Cancel01Icon,
  ArrowDown01Icon,
  ArrowRight01Icon,
  Search01Icon,
  InformationCircleIcon,
  CheckmarkCircle02Icon,
  Alert02Icon,
  AlertCircleIcon as HugeAlertCircleIcon,
  Copy01Icon,
  Loading03Icon,
  Upload04Icon,
} from '@hugeicons/core-free-icons'

type IconProps = Omit<React.ComponentProps<typeof HugeiconsIcon>, 'icon'>

const icon = (glyph: IconSvgElement, name: string) => {
  const Icon = (props: IconProps) => <HugeiconsIcon icon={glyph} {...props} />
  Icon.displayName = name
  return Icon
}

export const CheckIcon = icon(Tick02Icon, 'CheckIcon')
export const MinusIcon = icon(MinusSignIcon, 'MinusIcon')
export const CloseIcon = icon(Cancel01Icon, 'CloseIcon')
export const ChevronDownIcon = icon(ArrowDown01Icon, 'ChevronDownIcon')
export const ChevronRightIcon = icon(ArrowRight01Icon, 'ChevronRightIcon')
export const SearchIcon = icon(Search01Icon, 'SearchIcon')
export const InfoIcon = icon(InformationCircleIcon, 'InfoIcon')
export const CheckCircleIcon = icon(CheckmarkCircle02Icon, 'CheckCircleIcon')
export const AlertTriangleIcon = icon(Alert02Icon, 'AlertTriangleIcon')
export const AlertCircleIcon = icon(HugeAlertCircleIcon, 'AlertCircleIcon')
export const CopyIcon = icon(Copy01Icon, 'CopyIcon')
export const SpinnerIcon = icon(Loading03Icon, 'SpinnerIcon')
export const UploadIcon = icon(Upload04Icon, 'UploadIcon')
