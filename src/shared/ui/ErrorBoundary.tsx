import { Component, type ReactNode } from "react";

type Props = { children: ReactNode };
type State = { error: Error | null };

/// Last-resort net around the whole window: an unexpected render crash used to
/// leave a blank white page with no way back short of quitting the app. This
/// catches it and offers a reload instead — nothing here can save the app from
/// a real bug, but it turns "force-quit and hope" into one click.
export class ErrorBoundary extends Component<Props, State> {
  state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  componentDidCatch(error: Error, info: { componentStack: string }) {
    console.error("[ErrorBoundary]", error, info.componentStack);
  }

  render() {
    if (!this.state.error) return this.props.children;
    // Shown on screen (not just logged to a devtools console the user has no
    // easy way to open) so a screenshot of this screen is enough to diagnose
    // the crash — no need to ask "what did it say" after the fact.
    const detail = `${this.state.error.name}: ${this.state.error.message}`;
    return (
      <div style={{
        display: "flex", flexDirection: "column", alignItems: "center", justifyContent: "center",
        height: "100vh", gap: 12, padding: 24, textAlign: "center", background: "#0b0b0d", color: "#e5e5e5",
        fontFamily: "system-ui, sans-serif",
      }}>
        <div style={{ fontSize: 15, fontWeight: 600 }}>Đã xảy ra lỗi hiển thị</div>
        <div style={{ fontSize: 13, color: "#9a9a9a", maxWidth: 480 }}>
          Một phần giao diện gặp lỗi. Bấm nút dưới để tải lại — dữ liệu profile không bị ảnh hưởng.
        </div>
        <div style={{
          maxWidth: 560, maxHeight: 140, overflow: "auto", fontFamily: "monospace", fontSize: 11,
          color: "#c9c9c9", background: "#1a1a1d", borderRadius: 8, padding: "8px 12px", textAlign: "left",
          whiteSpace: "pre-wrap", wordBreak: "break-word",
        }}>
          {detail}
        </div>
        <button
          onClick={() => window.location.reload()}
          style={{
            marginTop: 8, padding: "8px 16px", borderRadius: 8, border: "none",
            background: "#f97316", color: "#fff", fontSize: 13, fontWeight: 600, cursor: "pointer",
          }}
        >
          Tải lại
        </button>
      </div>
    );
  }
}
