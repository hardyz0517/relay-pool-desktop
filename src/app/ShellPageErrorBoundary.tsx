import { Component, type ErrorInfo, type ReactNode } from "react";
import { Button } from "@/components/ui";
import { recordFrontendBoundaryFailure } from "@/lib/bridge/generated";

type Props = { children: ReactNode };
type State = { failed: boolean };

export class ShellPageErrorBoundary extends Component<Props, State> {
  state: State = { failed: false };

  static getDerivedStateFromError(): State {
    return { failed: true };
  }

  componentDidCatch(_error: Error, _info: ErrorInfo) {
    // The boundary event intentionally carries no error, stack, props, or DOM data.
    void recordFrontendBoundaryFailure().catch(() => undefined);
  }

  private retry = () => this.setState({ failed: false });

  render() {
    if (!this.state.failed) {
      return this.props.children;
    }

    return (
      <div className="flex min-h-full items-center justify-center p-6" role="alert">
        <div className="grid max-w-sm gap-3 text-center">
          <h2 className="text-base font-semibold text-foreground">页面加载失败</h2>
          <p className="text-sm text-muted-foreground">可以重试，或从侧边栏切换到其他页面。</p>
          <Button className="justify-self-center" onClick={this.retry} variant="secondary">
            重试
          </Button>
        </div>
      </div>
    );
  }
}
