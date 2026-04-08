import { Component, type ErrorInfo, type ReactNode } from 'react';

interface AppErrorBoundaryProps {
  children: ReactNode;
}

interface AppErrorBoundaryState {
  error: Error | null;
  componentStack: string | null;
}

export class AppErrorBoundary extends Component<
  AppErrorBoundaryProps,
  AppErrorBoundaryState
> {
  state: AppErrorBoundaryState = {
    error: null,
    componentStack: null,
  };

  static getDerivedStateFromError(error: Error): AppErrorBoundaryState {
    return {
      error,
      componentStack: null,
    };
  }

  componentDidCatch(error: Error, errorInfo: ErrorInfo) {
    console.error('Nazh UI crashed inside React render tree', error, errorInfo);
    this.setState({
      error,
      componentStack: errorInfo.componentStack || null,
    });
  }

  render() {
    const { error, componentStack } = this.state;
    if (!error) {
      return this.props.children;
    }

    return (
      <main className="app-error-boundary">
        <section className="app-error-boundary__panel">
          <span className="app-error-boundary__eyebrow">Nazh Error Boundary</span>
          <h1>界面异常已拦截</h1>
          <p>{error.message || '渲染链发生未知异常。'}</p>
          <div className="app-error-boundary__actions">
            <button type="button" onClick={() => window.location.reload()}>
              重新加载应用
            </button>
          </div>
          {componentStack ? (
            <pre className="app-error-boundary__stack">{componentStack}</pre>
          ) : null}
        </section>
      </main>
    );
  }
}
