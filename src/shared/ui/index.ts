/// Shared UI — thin FSD facade over the Shardx UI Kit plus app-specific
/// wrappers. Import UI primitives from here, not from the kit directly.
export {
  Button,
  Tag,
  Alert,
  Input,
  Textarea,
  Checkbox,
  Radio,
  Switch,
  Select,
  Slider,
  Tabs,
  SegmentControl,
  Breadcrumb,
  ProgressBar,
  Tooltip,
  Modal,
  AlertModal,
  DialogModal,
  Pagination,
  cn,
} from "@proxyshard/shardx-ui-kit";
export type {
  ButtonProps,
  SelectOption,
  SegmentItem,
  TabItem,
  BreadcrumbItem,
} from "@proxyshard/shardx-ui-kit";

export { default as Badge } from "./Badge";
export type { BadgeProps, BadgeColor, BadgeVariant, BadgeSize } from "./Badge";

export { Field } from "./Field";
export { NumField } from "./NumField";
export { SelectField } from "./SelectField";
export { CSSelect } from "./CSSelect";
export { Pair } from "./Pair";
export { PortList } from "./PortList";
export { CopyField } from "./CopyField";
export { CountryFlag } from "./CountryFlag";
export { Metric } from "./Metric";
export { Topbar } from "./Topbar";
