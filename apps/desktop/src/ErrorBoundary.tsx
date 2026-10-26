import { Component, type ErrorInfo, type ReactNode } from 'react';
import { desktopBridge, nativeAvailable } from './bridge';

interface Props {
  children: ReactNode;
}

interface State {
  error?: Error;
}

export class ErrorBoundary extends Component<Props, State> {
  state: State = {};

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    if (nativeAvailable()) {
      void desktopBridge.logUiError(
        `${error.message}\n${info.componentStack ?? ''}`,
      );
    }
  }

  render() {
    if (!this.state.error) return this.props.children;
    return (
      <main className="recovery-view" role="alert">
        <h1>PIMP could not display the interface</h1>
        <p>
          Your media and catalogue have not been changed. Reload the interface
          to reconnect to the native application.
        </p>
        <details>
          <summary>Technical detail</summary>
          <code>{this.state.error.message}</code>
        </details>
        <button className="primary" onClick={() => window.location.reload()}>
          Reload interface
        </button>
      </main>
    );
  }
}
