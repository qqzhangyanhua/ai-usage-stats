import { Component, type ErrorInfo, type ReactNode } from "react";
import { Icon } from "../icons";
import { Button } from "./ui/Button";

interface Props {
  children: ReactNode;
  fullscreen?: boolean;
}

interface State {
  error: Error | null;
}

export class ErrorBoundary extends Component<Props, State> {
  state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error("渲染异常已被 ErrorBoundary 捕获：", error, info.componentStack);
  }

  handleReset = () => {
    this.setState({ error: null });
  };

  render() {
    const { error } = this.state;
    if (!error) {
      return this.props.children;
    }
    return (
      <div className={this.props.fullscreen === false ? "error-boundary inline" : "error-boundary"}>
        <div className="error-boundary-card">
          <Icon name="alertTriangle" size={26} className="error-boundary-icon" />
          <h2>页面出现异常</h2>
          <p className="muted">界面渲染时发生了一个未预期的错误，你可以尝试重新加载。</p>
          <pre className="error-boundary-detail">{error.message}</pre>
          <div className="error-boundary-actions">
            <Button variant="accent" onClick={this.handleReset}>
              重试
            </Button>
            <Button onClick={() => window.location.reload()}>刷新应用</Button>
          </div>
        </div>
      </div>
    );
  }
}
