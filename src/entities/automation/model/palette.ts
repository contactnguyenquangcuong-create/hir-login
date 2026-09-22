/** The block library: what an operator can add by hand, grouped the way the
 *  picker shows it. Recording from the live browser adds the same kinds.
 *
 *  `label`, `about` and `hint` hold TRANSLATION KEYS, not text — the English
 *  lives in shared/i18n/locales/en.json under the same key. Whatever renders
 *  one puts it through t() first; searching does the same, so a person types
 *  what they see. */

export type ParamKind =
  | "text"
  | "number"
  | "url"
  | "key"
  | "select"
  | "textarea"
  | "resiloc"
  /** A saved project, chosen from a list. */
  | "project"
  /** An entry point inside the project another param names. */
  | "projectStep";

export type ParamSpec = {
  name: string;
  label: string;
  kind: ParamKind;
  /** Shown when the field is empty. */
  hint?: string;
  options?: string[];
  default?: string | number;
  /** For a dependent picker: the sibling param whose value it reads. */
  of?: string;
  /** Can hold something that must not leave the machine — a password, a token,
   *  a proxy with credentials in it. Only these fields offer "secret", which
   *  blanks the value on export. A folder name or a CSS selector does not. */
  secret?: boolean;
};

export type BlockSpec = {
  kind: string;
  label: string;
  /** One line, shown under the name in the picker. */
  about: string;
  params: ParamSpec[];
};

export type Category = { id: string; label: string; blocks: BlockSpec[] };

export const PALETTE: Category[] = [
  {
    id: "profile",
    label: "palette.cat.profile",
    blocks: [
      {
        kind: "profile.temp",
        label: "palette.profileTemp.label",
        about: "palette.profileTemp.about",
        params: [
          { name: "platform", label: "palette.profileTemp.platform.label", kind: "select", options: ["", "windows", "macos", "linux", "android"] },
          { name: "proxy", label: "palette.profileTemp.proxy.label", kind: "text", hint: "palette.profileTemp.proxy.hint", secret: true },
          { name: "into", label: "palette.profileTemp.into.label", kind: "text", hint: "palette.profileTemp.into.hint" },
        ],
      },
      {
        kind: "profile.create",
        label: "palette.profileCreate.label",
        about: "palette.profileCreate.about",
        params: [
          { name: "name", label: "palette.profileCreate.name.label", kind: "text", hint: "palette.profileCreate.name.hint" },
          { name: "folder", label: "palette.profileCreate.folder.label", kind: "text", default: "Automation" },
          { name: "platform", label: "palette.profileCreate.platform.label", kind: "select", options: ["", "windows", "macos", "linux", "android"] },
          { name: "proxy", label: "palette.profileCreate.proxy.label", kind: "text", hint: "palette.profileCreate.proxy.hint", secret: true },
          { name: "into", label: "palette.profileCreate.into.label", kind: "text", hint: "palette.profileCreate.into.hint" },
        ],
      },
      {
        kind: "proxy.swap",
        label: "palette.proxySwap.label",
        about:
          "Changes the profile's proxy without restarting the browser (through the core; keeps the WebRTC/QUIC UDP relay). Leave empty for a direct connection.",
        params: [
          { name: "proxy", label: "palette.proxySwap.proxy.label", kind: "text", hint: "palette.proxySwap.proxy.hint", secret: true },
        ],
      },
      {
        kind: "profile.use",
        label: "palette.profileUse.label",
        about: "palette.profileUse.about",
        params: [{ name: "id", label: "palette.profileUse.id.label", kind: "text", hint: "palette.profileUse.id.hint" }],
      },
      {
        kind: "profile.keep",
        label: "palette.profileKeep.label",
        about: "palette.profileKeep.about",
        params: [{ name: "folder", label: "palette.profileKeep.folder.label", kind: "text", hint: "palette.profileKeep.folder.hint" }],
      },
      {
        kind: "profile.read",
        label: "palette.profileRead.label",
        about: "palette.profileRead.about",
        params: [
          {
            name: "path",
            label: "palette.profileRead.label",
            kind: "text",
            hint: "palette.text.path.hint",
          },
          { name: "into", label: "palette.text.into.label", kind: "text", hint: "palette.text.into.hint" },
        ],
      },
      {
        kind: "profile.delete",
        label: "palette.profileDelete.label",
        about: "palette.profileDelete.about",
        params: [{ name: "id", label: "palette.profileDelete.id.label", kind: "text", hint: "palette.profileDelete.id.hint" }],
      },
    ],
  },
  {
    id: "proxy",
    label: "palette.cat.proxy",
    blocks: [
      {
        kind: "proxy.set",
        label: "palette.proxySet.label",
        about: "palette.proxySet.about",
        params: [{ name: "proxy", label: "palette.proxySet.proxy.label", kind: "text", hint: "palette.proxySet.proxy.hint", secret: true }],
      },
      {
        kind: "proxy.residential",
        label: "palette.proxyResidential.label",
        about:
          "Builds a ProxyShard residential session the way the generator card does, and puts it on the profile. Stops the run if the plan has no traffic left.",
        params: [
          {
            name: "plan",
            label: "palette.proxyResidential.label",
            kind: "select",
            options: ["standart", "premium", "unmetered"],
            default: "standart",
          },
          { name: "country", label: "palette.select.country.label", kind: "resiloc", hint: "palette.select.country.hint" },
          { name: "region", label: "palette.select.region.label", kind: "resiloc", hint: "palette.select.region.hint" },
          { name: "city", label: "palette.select.city.label", kind: "resiloc", hint: "palette.select.city.hint" },
          { name: "isp", label: "palette.select.isp.label", kind: "resiloc", hint: "palette.select.isp.hint" },
          {
            name: "os",
            label: "palette.select.label",
            kind: "select",
            options: ["", "macos", "windows", "android", "linux", "ios"],
            hint: "palette.select.os.hint",
          },
          {
            name: "session",
            label: "palette.select.label",
            kind: "select",
            options: ["sticky", "dynamic"],
            default: "sticky",
            hint: "palette.select.hint.hint",
          },
          {
            name: "session_mode",
            label: "palette.select.label",
            kind: "select",
            options: ["after 5 sec", "static"],
            default: "after 5 sec",
          },
          {
            name: "protocol",
            label: "palette.select.label",
            kind: "select",
            options: ["socks5", "http"],
            default: "socks5",
          },
          { name: "relay", label: "palette.select.relay.label", kind: "text", hint: "palette.select.relay.hint" },
          { name: "into", label: "palette.select.into.label", kind: "text", hint: "palette.select.into.hint" },
        ],
      },
      {
        kind: "proxy.generate",
        label: "palette.proxyGenerate.label",
        about: "palette.proxyGenerate.about",
        params: [
          { name: "order", label: "palette.proxyGenerate.order.label", kind: "text", hint: "palette.proxyGenerate.order.hint" },
          { name: "country", label: "palette.proxyGenerate.country.label", kind: "text", hint: "palette.proxyGenerate.country.hint" },
          { name: "into", label: "palette.proxyGenerate.into.label", kind: "text", hint: "palette.proxyGenerate.into.hint" },
        ],
      },
    ],
  },
  {
    id: "navigation",
    label: "palette.cat.navigation",
    blocks: [
      {
        kind: "goto",
        label: "palette.goto.label",
        about: "palette.goto.about",
        params: [{ name: "url", label: "palette.goto.url.label", kind: "url", hint: "palette.goto.url.hint" }],
      },
      { kind: "back", label: "palette.back.label", about: "palette.back.about", params: [] },
      { kind: "forward", label: "palette.forward.label", about: "palette.forward.about", params: [] },
      { kind: "reload", label: "palette.reload.label", about: "palette.reload.about", params: [] },
      {
        kind: "waitLoad",
        label: "palette.waitload.label",
        about: "palette.waitload.about",
        params: [{ name: "timeout", label: "palette.waitload.timeout.label", kind: "number", default: 30 }],
      },
    ],
  },
  {
    // The phone-only steps: gestures a cursor cannot make, plus turning the
    // handset. The ordinary Pointer steps already become touches on a phone
    // profile — these are the ones with no desktop twin at all.
    id: "touch",
    label: "palette.cat.touch",
    blocks: [
      {
        kind: "touch.tap",
        label: "palette.touchTap.label",
        about: "palette.touchTap.about",
        params: [
          { name: "selector", label: "palette.touchTap.selector.label", kind: "text", hint: "palette.touchTap.selector.hint" },
          { name: "x", label: "palette.touchTap.x.label", kind: "number" },
          { name: "y", label: "palette.touchTap.y.label", kind: "number" },
          { name: "tapCount", label: "palette.touchTap.tapcount.label", kind: "number", default: 1 },
          { name: "timeout", label: "palette.touchTap.timeout.label", kind: "number", default: 5 },
        ],
      },
      {
        kind: "touch.longPress",
        label: "palette.touchLongPress.label",
        about: "palette.touchLongPress.about",
        params: [
          { name: "selector", label: "palette.touchLongPress.selector.label", kind: "text", hint: "palette.touchLongPress.selector.hint" },
          { name: "x", label: "palette.touchLongPress.x.label", kind: "number" },
          { name: "y", label: "palette.touchLongPress.y.label", kind: "number" },
          { name: "timeout", label: "palette.touchLongPress.timeout.label", kind: "number", default: 5 },
        ],
      },
      {
        kind: "touch.swipe",
        label: "palette.touchSwipe.label",
        about: "palette.touchSwipe.about",
        params: [
          { name: "selector", label: "palette.touchSwipe.selector.label", kind: "text", hint: "palette.touchSwipe.selector.hint" },
          { name: "dx", label: "palette.touchSwipe.dx.label", kind: "number", default: 0 },
          { name: "dy", label: "palette.touchSwipe.dy.label", kind: "number", default: 0 },
          { name: "timeout", label: "palette.touchSwipe.timeout.label", kind: "number", default: 5 },
        ],
      },
      {
        kind: "touch.drag",
        label: "palette.touchDrag.label",
        about: "palette.touchDrag.about",
        params: [
          { name: "selector", label: "palette.touchDrag.selector.label", kind: "text", hint: "palette.touchDrag.selector.hint" },
          { name: "to", label: "palette.touchDrag.to.label", kind: "text", hint: "palette.touchDrag.to.hint" },
          { name: "holdMs", label: "palette.touchDrag.holdms.label", kind: "number", hint: "palette.touchDrag.holdms.hint" },
          { name: "timeout", label: "palette.touchDrag.timeout.label", kind: "number", default: 5 },
        ],
      },
      {
        kind: "touch.pinch",
        label: "palette.touchPinch.label",
        about: "palette.touchPinch.about",
        params: [
          { name: "scale", label: "palette.touchPinch.scale.label", kind: "number", default: 2 },
          { name: "selector", label: "palette.touchPinch.selector.label", kind: "text", hint: "palette.touchPinch.selector.hint" },
          { name: "x", label: "palette.touchPinch.x.label", kind: "number" },
          { name: "y", label: "palette.touchPinch.y.label", kind: "number" },
        ],
      },
      {
        kind: "screen.rotate",
        label: "palette.screenRotate.label",
        about: "palette.screenRotate.about",
        params: [
          { name: "angle", label: "palette.screenRotate.angle.label", kind: "number", default: 90, hint: "palette.screenRotate.angle.hint" },
          { name: "turnMs", label: "palette.screenRotate.turnms.label", kind: "number", hint: "palette.screenRotate.turnms.hint" },
          { name: "into", label: "palette.screenRotate.into.label", kind: "text", hint: "palette.screenRotate.into.hint" },
        ],
      },
    ],
  },
  {
    id: "pointer",
    label: "palette.cat.pointer",
    blocks: [
      {
        kind: "click",
        label: "palette.click.label",
        about: "palette.click.about",
        params: [{ name: "selector", label: "palette.click.selector.label", kind: "text", hint: "palette.click.selector.hint" },
          { name: "timeout", label: "palette.click.timeout.label", kind: "number", default: 5 },
        ],
      },
      {
        kind: "doubleClick",
        label: "palette.doubleclick.label",
        about: "palette.doubleclick.about",
        params: [{ name: "selector", label: "palette.doubleclick.selector.label", kind: "text", hint: "palette.doubleclick.selector.hint" },
          { name: "timeout", label: "palette.doubleclick.timeout.label", kind: "number", default: 5 },
        ],
      },
      {
        kind: "rightClick",
        label: "palette.rightclick.label",
        about: "palette.rightclick.about",
        params: [{ name: "selector", label: "palette.rightclick.selector.label", kind: "text", hint: "palette.rightclick.selector.hint" },
          { name: "timeout", label: "palette.rightclick.timeout.label", kind: "number", default: 5 },
        ],
      },
      {
        kind: "hover",
        label: "palette.hover.label",
        about: "palette.hover.about",
        params: [{ name: "selector", label: "palette.hover.selector.label", kind: "text", hint: "palette.hover.selector.hint" },
          { name: "timeout", label: "palette.hover.timeout.label", kind: "number", default: 5 },
        ],
      },
      {
        kind: "drag",
        label: "palette.drag.label",
        about: "palette.drag.about",
        params: [
          { name: "selector", label: "palette.drag.selector.label", kind: "text", hint: "palette.drag.selector.hint" },
          { name: "to", label: "palette.drag.to.label", kind: "text", hint: "palette.drag.to.hint" },
          {
            name: "button",
            label: "palette.drag.label",
            kind: "select",
            options: ["left", "middle", "right"],
            default: "left",
          },
          { name: "holdMs", label: "palette.select.holdms.label", kind: "number", hint: "palette.select.holdms.hint" },
          { name: "timeout", label: "palette.select.timeout.label", kind: "number", default: 5 },
        ],
      },
      {
        kind: "swipe",
        label: "palette.swipe.label",
        about:
          "Presses and moves by a distance, then lets go — a slider, a canvas, a sweep that selects text. Starts in the middle of the page when no element is named.",
        params: [
          { name: "selector", label: "palette.swipe.selector.label", kind: "text", hint: "palette.swipe.selector.hint" },
          { name: "dx", label: "palette.swipe.dx.label", kind: "number", default: 0 },
          { name: "dy", label: "palette.swipe.dy.label", kind: "number", default: 0 },
          {
            name: "button",
            label: "palette.swipe.label",
            kind: "select",
            options: ["left", "middle", "right"],
            default: "left",
          },
          { name: "timeout", label: "palette.select.timeout.label", kind: "number", default: 5 },
        ],
      },
      {
        kind: "scroll",
        label: "palette.scroll.label",
        about: "palette.scroll.about",
        params: [
          { name: "deltaY", label: "palette.scroll.deltay.label", kind: "number", default: 600 },
          { name: "deltaX", label: "palette.scroll.deltax.label", kind: "number", default: 0 },
          { name: "timeout", label: "palette.scroll.timeout.label", kind: "number", default: 5 },
        ],
      },
    ],
  },
  {
    id: "keyboard",
    label: "palette.cat.keyboard",
    blocks: [
      {
        kind: "type",
        label: "palette.type.label",
        about: "palette.type.about",
        params: [
          { name: "text", label: "palette.type.text.label", kind: "text", hint: "palette.type.text.hint", secret: true },
          { name: "selector", label: "palette.type.selector.label", kind: "text", hint: "palette.type.selector.hint" },
          { name: "timeout", label: "palette.type.timeout.label", kind: "number", default: 5 },
        ],
      },
      {
        kind: "press",
        label: "palette.press.label",
        about: "palette.press.about",
        params: [
          { name: "key", label: "palette.press.key.label", kind: "key", hint: "palette.press.key.hint" },
          { name: "modifiers", label: "palette.press.modifiers.label", kind: "text", hint: "palette.press.modifiers.hint" },
        ],
      },
      {
        kind: "clear",
        label: "palette.clear.label",
        about: "palette.clear.about",
        params: [{ name: "selector", label: "palette.clear.selector.label", kind: "text", hint: "palette.clear.selector.hint" },
          { name: "timeout", label: "palette.clear.timeout.label", kind: "number", default: 5 },
        ],
      },
    ],
  },
  {
    id: "flow",
    label: "palette.cat.flow",
    blocks: [
      {
        kind: "flow.call",
        label: "palette.flowCall.label",
        about: "palette.flowCall.about",
        params: [
          { name: "project", label: "palette.flowCall.project.label", kind: "project" },
          { name: "entry", label: "palette.flowCall.entry.label", kind: "projectStep", of: "project", hint: "palette.flowCall.entry.hint" },
          { name: "in", label: "palette.flowCall.in.label", kind: "textarea", hint: "palette.flowCall.in.hint" },
          { name: "out", label: "palette.flowCall.out.label", kind: "text", hint: "palette.flowCall.out.hint" },
        ],
      },
      {
        kind: "flow.entry",
        label: "palette.flowEntry.label",
        about: "palette.flowEntry.about",
        params: [
          { name: "name", label: "palette.flowEntry.name.label", kind: "text", hint: "palette.flowEntry.name.hint" },
          { name: "about", label: "palette.flowEntry.about.label", kind: "text" },
        ],
      },
      {
        kind: "wait",
        label: "palette.wait.label",
        about: "palette.wait.about",
        params: [{ name: "seconds", label: "palette.wait.seconds.label", kind: "number", default: 2 }],
      },
      {
        kind: "waitFor",
        label: "palette.waitfor.label",
        about: "palette.waitfor.about",
        params: [
          { name: "selector", label: "palette.waitfor.selector.label", kind: "text", hint: "palette.waitfor.selector.hint" },
          { name: "timeout", label: "palette.waitfor.timeout.label", kind: "number", default: 15 },
        ],
      },
      {
        kind: "waitGone",
        label: "palette.waitgone.label",
        about: "palette.waitgone.about",
        params: [
          { name: "selector", label: "palette.waitgone.selector.label", kind: "text", hint: "palette.waitgone.selector.hint" },
          { name: "timeout", label: "palette.waitgone.timeout.label", kind: "number", default: 15 },
        ],
      },
      {
        kind: "waitText",
        label: "palette.waittext.label",
        about: "palette.waittext.about",
        params: [
          { name: "selector", label: "palette.waittext.selector.label", kind: "text", hint: "palette.waittext.selector.hint" },
          { name: "text", label: "palette.waittext.text.label", kind: "text", hint: "palette.waittext.text.hint" },
          { name: "timeout", label: "palette.waittext.timeout.label", kind: "number", default: 15 },
        ],
      },
      {
        kind: "waitUrl",
        label: "palette.waiturl.label",
        about: "palette.waiturl.about",
        params: [
          { name: "url", label: "palette.waiturl.url.label", kind: "text", hint: "palette.waiturl.url.hint" },
          { name: "timeout", label: "palette.waiturl.timeout.label", kind: "number", default: 30 },
        ],
      },
      {
        kind: "ifExists",
        label: "palette.ifexists.label",
        about: "palette.ifexists.about",
        params: [{ name: "selector", label: "palette.ifexists.selector.label", kind: "text", hint: "palette.ifexists.selector.hint" }],
      },
      {
        kind: "if.value",
        label: "palette.ifValue.label",
        about:
          "Branches on two values. True → “when it works”; false → “when it fails” (your else). Set both branches in the step's panel.",
        params: [
          { name: "a", label: "palette.ifValue.a.label", kind: "text", hint: "palette.ifValue.a.hint" },
          {
            name: "op",
            label: "palette.ifValue.label",
            kind: "select",
            options: ["=", "≠", "contains", "not contains", "starts with", "ends with", "is empty", "is not empty", ">", ">=", "<", "<="],
            default: "=",
          },
          { name: "b", label: "palette.select.b.label", kind: "text", hint: "palette.select.b.hint" },
        ],
      },
      {
        kind: "if.exists",
        label: "palette.ifExists.label",
        about: "palette.ifExists.about",
        params: [{ name: "selector", label: "palette.ifExists.selector.label", kind: "text", hint: "palette.ifExists.selector.hint" }],
      },
      {
        kind: "if.text",
        label: "palette.ifText.label",
        about: "palette.ifText.about",
        params: [
          { name: "selector", label: "palette.ifText.selector.label", kind: "text", hint: "palette.ifText.selector.hint" },
          {
            name: "op",
            label: "palette.ifText.label",
            kind: "select",
            options: ["contains", "not contains", "=", "≠", "starts with", "ends with", "is empty", "is not empty"],
            default: "contains",
          },
          { name: "text", label: "palette.select.text.label", kind: "text", hint: "palette.select.text.hint" },
        ],
      },
      {
        kind: "if.url",
        label: "palette.ifUrl.label",
        about: "palette.ifUrl.about",
        params: [
          {
            name: "op",
            label: "palette.ifUrl.label",
            kind: "select",
            options: ["contains", "not contains", "starts with", "ends with", "="],
            default: "contains",
          },
          { name: "text", label: "palette.select.text.label", kind: "text", hint: "palette.select.text.hint" },
        ],
      },
      { kind: "stop", label: "palette.stop.label", about: "palette.stop.about", params: [] },
    ],
  },
  {
    id: "requests",
    label: "palette.cat.requests",
    blocks: [
      {
        kind: "script.run",
        label: "palette.scriptRun.label",
        about:
          "Runs your JS in the page through the core's isolated world — no page-visible trace (not Runtime.evaluate). Pick the page's own world only when the script must read the page's own variables.",
        params: [
          { name: "source", label: "palette.scriptRun.source.label", kind: "textarea", hint: "palette.scriptRun.source.hint" },
          { name: "file", label: "palette.scriptRun.file.label", kind: "text", hint: "palette.scriptRun.file.hint" },
          { name: "world", label: "palette.scriptRun.world.label", kind: "select", options: ["isolated", "main"], default: "isolated" },
          { name: "into", label: "palette.scriptRun.into.label", kind: "text", hint: "palette.scriptRun.into.hint" },
        ],
      },
      {
        kind: "http.request",
        label: "palette.httpRequest.label",
        about:
          "Sends it from the launcher, not from the page — with a chosen TLS fingerprint and, by default, through the profile's own proxy.",
        params: [
          {
            name: "method",
            label: "palette.httpRequest.label",
            kind: "select",
            options: ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"],
            default: "GET",
          },
          { name: "url", label: "palette.select.url.label", kind: "url", hint: "palette.select.url.hint" },
          {
            name: "headers",
            label: "palette.select.label",
            kind: "text",
            hint: "palette.text.headers.hint",
          },
          { name: "body", label: "palette.text.body.label", kind: "text", hint: "palette.text.body.hint", secret: true },
          {
            name: "fingerprint",
            label: "palette.text.label",
            kind: "select",
            hint: "palette.select.fingerprint.hint",
            options: [
              "",
              "chrome_149", "chrome_148", "chrome_147", "chrome_146", "chrome_145",
              "chrome_144", "chrome_143", "chrome_142", "chrome_141", "chrome_140",
              "firefox_151", "firefox_150", "firefox_149", "firefox_148", "firefox_147",
              "firefox_146", "firefox_145", "firefox_144", "firefox_143", "firefox_142",
              "safari_26.4", "safari_26", "safari_18.5", "safari_18.3.1", "safari_18",
              "safari_ios_26.2", "safari_ios_18.1.1", "safari_ipad_26.2",
              "edge_148", "edge_147", "edge_146", "edge_145", "edge_144",
              "edge_143", "edge_142", "edge_141", "edge_140",
              "opera_131", "opera_130", "opera_129",
              "okhttp_5", "okhttp_4.12",
            ],
          },
          {
            name: "session",
            label: "palette.select.label",
            kind: "text",
            hint: "palette.text.session.hint",
          },
          {
            name: "via",
            label: "palette.text.label",
            kind: "select",
            options: ["profile", "host"],
            default: "profile",
          },
          { name: "into", label: "palette.select.into.label", kind: "text", hint: "palette.select.into.hint" },
          { name: "timeout", label: "palette.select.timeout.label", kind: "number", default: 30 },
        ],
      },
      {
        kind: "http.endSession",
        label: "palette.httpEndSession.label",
        about: "palette.httpEndSession.about",
        params: [{ name: "session", label: "palette.httpEndSession.session.label", kind: "text" }],
      },
    ],
  },
  {
    id: "traffic",
    label: "palette.cat.traffic",
    blocks: [
      {
        kind: "traffic.observe",
        label: "palette.trafficObserve.label",
        about:
          "Starts capturing this profile's requests so you can open them in the Traffic tab and seed rules from the real response. Turn it off to stop.",
        params: [
          { name: "enabled", label: "palette.trafficObserve.enabled.label", kind: "select", options: ["on", "off"], default: "on" },
        ],
      },
      {
        kind: "traffic.block",
        label: "palette.trafficBlock.label",
        about: "palette.trafficBlock.about",
        params: [
          { name: "url", label: "palette.trafficBlock.url.label", kind: "text", hint: "palette.trafficBlock.url.hint" },
          { name: "method", label: "palette.trafficBlock.method.label", kind: "text", hint: "palette.trafficBlock.method.hint" },
          {
            name: "resource",
            label: "palette.trafficBlock.label",
            kind: "select",
            options: ["any", "document", "xhr", "script", "stylesheet", "image", "font", "media", "other"],
            default: "any",
          },
          { name: "reason", label: "palette.select.reason.label", kind: "text", hint: "palette.select.reason.hint" },
        ],
      },
      {
        kind: "traffic.redirect",
        label: "palette.trafficRedirect.label",
        about:
          "Sends a matching request to another address, transparently — the page still sees the original URL.",
        params: [
          { name: "url", label: "palette.trafficRedirect.url.label", kind: "text", hint: "palette.trafficRedirect.url.hint" },
          { name: "to", label: "palette.trafficRedirect.to.label", kind: "url", hint: "palette.trafficRedirect.to.hint" },
          { name: "method", label: "palette.trafficRedirect.method.label", kind: "text", hint: "palette.trafficRedirect.method.hint" },
          {
            name: "resource",
            label: "palette.trafficRedirect.label",
            kind: "select",
            options: ["any", "document", "xhr", "script", "stylesheet", "image", "font", "media", "other"],
            default: "any",
          },
        ],
      },
      {
        kind: "traffic.setHeaders",
        label: "palette.trafficSetHeaders.label",
        about: "palette.trafficSetHeaders.about",
        params: [
          { name: "url", label: "palette.trafficSetHeaders.url.label", kind: "text", hint: "palette.trafficSetHeaders.url.hint" },
          { name: "headers", label: "palette.trafficSetHeaders.headers.label", kind: "textarea", hint: "palette.trafficSetHeaders.headers.hint" },
          { name: "setBody", label: "palette.trafficSetHeaders.setbody.label", kind: "textarea", hint: "palette.trafficSetHeaders.setbody.hint" },
          { name: "setMethod", label: "palette.trafficSetHeaders.setmethod.label", kind: "text", hint: "palette.trafficSetHeaders.setmethod.hint" },
          {
            name: "resource",
            label: "palette.trafficSetHeaders.label",
            kind: "select",
            options: ["any", "document", "xhr", "script", "stylesheet", "image", "font", "media", "other"],
            default: "any",
          },
        ],
      },
      {
        kind: "traffic.editResponse",
        label: "palette.trafficEditResponse.label",
        about: "palette.trafficEditResponse.about",
        params: [
          { name: "url", label: "palette.trafficEditResponse.url.label", kind: "text", hint: "palette.trafficEditResponse.url.hint" },
          { name: "status", label: "palette.trafficEditResponse.status.label", kind: "number", hint: "palette.trafficEditResponse.status.hint" },
          { name: "responseHeaders", label: "palette.trafficEditResponse.responseheaders.label", kind: "textarea", hint: "palette.trafficEditResponse.responseheaders.hint" },
          { name: "responseBody", label: "palette.trafficEditResponse.responsebody.label", kind: "textarea", hint: "palette.trafficEditResponse.responsebody.hint" },
          {
            name: "resource",
            label: "palette.trafficEditResponse.label",
            kind: "select",
            options: ["any", "document", "xhr", "script", "stylesheet", "image", "font", "media", "other"],
            default: "any",
          },
        ],
      },
      {
        kind: "traffic.fulfill",
        label: "palette.trafficFulfill.label",
        about: "palette.trafficFulfill.about",
        params: [
          { name: "url", label: "palette.trafficFulfill.url.label", kind: "text", hint: "palette.trafficFulfill.url.hint" },
          { name: "status", label: "palette.trafficFulfill.status.label", kind: "number", default: 200 },
          { name: "responseHeaders", label: "palette.trafficFulfill.responseheaders.label", kind: "textarea", hint: "palette.trafficFulfill.responseheaders.hint" },
          { name: "responseBody", label: "palette.trafficFulfill.responsebody.label", kind: "textarea" },
          {
            name: "resource",
            label: "palette.trafficFulfill.label",
            kind: "select",
            options: ["any", "document", "xhr", "script", "stylesheet", "image", "font", "media", "other"],
            default: "any",
          },
        ],
      },
    ],
  },
  {
    id: "data",
    label: "palette.cat.data",
    blocks: [
      {
        kind: "log",
        label: "palette.log.label",
        about: "palette.log.about",
        params: [{ name: "text", label: "palette.log.text.label", kind: "text", hint: "palette.log.text.hint" }],
      },
      {
        kind: "readText",
        label: "palette.readtext.label",
        about: "palette.readtext.about",
        params: [
          { name: "selector", label: "palette.readtext.selector.label", kind: "text", hint: "palette.readtext.selector.hint" },
          { name: "into", label: "palette.readtext.into.label", kind: "text", hint: "palette.readtext.into.hint" },
          { name: "timeout", label: "palette.readtext.timeout.label", kind: "number", default: 5 },
        ],
      },
      {
        kind: "var.set",
        label: "palette.varSet.label",
        about: "palette.varSet.about",
        params: [
          { name: "name", label: "palette.varSet.name.label", kind: "text", hint: "palette.varSet.name.hint" },
          { name: "value", label: "palette.varSet.value.label", kind: "text", hint: "palette.varSet.value.hint", secret: true },
        ],
      },
      {
        kind: "var.math",
        label: "palette.varMath.label",
        about: "palette.varMath.about",
        params: [
          { name: "a", label: "palette.varMath.a.label", kind: "text", hint: "palette.varMath.a.hint" },
          { name: "op", label: "palette.varMath.op.label", kind: "select", options: ["+", "-", "*", "/", "%", "min", "max"], default: "+" },
          { name: "b", label: "palette.varMath.b.label", kind: "text", hint: "palette.varMath.b.hint" },
          { name: "into", label: "palette.varMath.into.label", kind: "text", hint: "palette.varMath.into.hint" },
        ],
      },
      {
        kind: "var.autofill",
        label: "palette.varAutofill.label",
        about: "palette.varAutofill.about",
        params: [
          { name: "field", label: "palette.varAutofill.field.label", kind: "text", hint: "palette.varAutofill.field.hint" },
          { name: "into", label: "palette.varAutofill.into.label", kind: "text", hint: "palette.varAutofill.into.hint" },
        ],
      },
      {
        kind: "count",
        label: "palette.count.label",
        about: "palette.count.about",
        params: [
          { name: "selector", label: "palette.count.selector.label", kind: "text", hint: "palette.count.selector.hint" },
          { name: "into", label: "palette.count.into.label", kind: "text", hint: "palette.count.into.hint" },
        ],
      },
      {
        kind: "readAttribute",
        label: "palette.readattribute.label",
        about: "palette.readattribute.about",
        params: [
          { name: "selector", label: "palette.readattribute.selector.label", kind: "text", hint: "palette.readattribute.selector.hint" },
          { name: "name", label: "palette.readattribute.name.label", kind: "text", hint: "palette.readattribute.name.hint" },
          { name: "into", label: "palette.readattribute.into.label", kind: "text", hint: "palette.readattribute.into.hint" },
          { name: "timeout", label: "palette.readattribute.timeout.label", kind: "number", default: 5 },
        ],
      },
      {
        kind: "screenshot",
        label: "palette.screenshot.label",
        about: "palette.screenshot.about",
        params: [
          { name: "path", label: "palette.screenshot.path.label", kind: "text", hint: "palette.screenshot.path.hint" },
        ],
      },
      {
        kind: "var.random",
        label: "palette.varRandom.label",
        about: "palette.varRandom.about",
        params: [
          { name: "list", label: "palette.varRandom.list.label", kind: "text", hint: "palette.varRandom.list.hint" },
          { name: "min", label: "palette.varRandom.min.label", kind: "text", default: "0" },
          { name: "max", label: "palette.varRandom.max.label", kind: "text", default: "100" },
          { name: "into", label: "palette.varRandom.into.label", kind: "text", hint: "palette.varRandom.into.hint" },
        ],
      },
      {
        kind: "file.readLine",
        label: "palette.fileReadLine.label",
        about: "palette.fileReadLine.about",
        params: [
          { name: "path", label: "palette.fileReadLine.path.label", kind: "text", hint: "palette.fileReadLine.path.hint" },
          { name: "into", label: "palette.fileReadLine.into.label", kind: "text", hint: "palette.fileReadLine.into.hint" },
          {
            name: "take",
            label: "palette.fileReadLine.label",
            kind: "select",
            options: ["remove the line", "keep"],
            default: "remove the line",
          },
        ],
      },
      {
        kind: "file.append",
        label: "palette.fileAppend.label",
        about: "palette.fileAppend.about",
        params: [
          { name: "path", label: "palette.fileAppend.path.label", kind: "text", hint: "palette.fileAppend.path.hint" },
          { name: "line", label: "palette.fileAppend.line.label", kind: "text", hint: "palette.fileAppend.line.hint" },
        ],
      },
    ],
  },
  {
    id: "database",
    label: "palette.cat.database",
    blocks: [
      {
        kind: "db.open",
        label: "palette.dbOpen.label",
        about:
          "Opens a database and keeps it under a name for the rest of the run. SQLite is a file (empty = in-memory); the others take a connection string.",
        params: [
          { name: "driver", label: "palette.dbOpen.driver.label", kind: "select", options: ["sqlite", "postgres", "mysql", "mariadb", "mongodb"], default: "sqlite" },
          { name: "target", label: "palette.dbOpen.target.label", kind: "text", hint: "palette.dbOpen.target.hint" },
          { name: "database", label: "palette.dbOpen.database.label", kind: "text", hint: "palette.dbOpen.database.hint" },
          { name: "name", label: "palette.dbOpen.name.label", kind: "text", default: "default" },
        ],
      },
      {
        kind: "db.exec",
        label: "palette.dbExec.label",
        about: "palette.dbExec.about",
        params: [
          { name: "name", label: "palette.dbExec.name.label", kind: "text", default: "default" },
          { name: "sql", label: "palette.dbExec.sql.label", kind: "textarea", hint: "palette.dbExec.sql.hint" },
          { name: "into", label: "palette.dbExec.into.label", kind: "text", hint: "palette.dbExec.into.hint" },
        ],
      },
      {
        kind: "db.query",
        label: "palette.dbQuery.label",
        about: "palette.dbQuery.about",
        params: [
          { name: "name", label: "palette.dbQuery.name.label", kind: "text", default: "default" },
          { name: "sql", label: "palette.dbQuery.sql.label", kind: "textarea", hint: "palette.dbQuery.sql.hint" },
          { name: "mode", label: "palette.dbQuery.mode.label", kind: "select", options: ["rows", "row", "value", "count"], default: "rows" },
          { name: "into", label: "palette.dbQuery.into.label", kind: "text", hint: "palette.dbQuery.into.hint" },
        ],
      },
      {
        kind: "db.close",
        label: "palette.dbClose.label",
        about: "palette.dbClose.about",
        params: [
          { name: "name", label: "palette.dbClose.name.label", kind: "text", default: "default" },
        ],
      },
    ],
  },
];

export function specFor(kind: string): BlockSpec | null {
  for (const c of PALETTE) {
    const b = c.blocks.find((x) => x.kind === kind);
    if (b) return b;
  }
  return null;
}
