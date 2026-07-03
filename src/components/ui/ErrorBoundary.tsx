import React, { Component, ErrorInfo, ReactNode } from 'react';

interface Props {
  children: ReactNode;
  label?: string;
}

interface State {
  hasError: boolean;
  error: Error | null;
}

export class ErrorBoundary extends Component<Props, State> {
  constructor(props: Props) {
    super(props);
    this.state = { hasError: false, error: null };
  }

  static getDerivedStateFromError(error: Error): State {
    return { hasError: true, error };
  }

  componentDidCatch(error: Error, errorInfo: ErrorInfo) {
    console.error(`[Debug:ErrorBoundary] ${this.props.label || 'Unknown'} caught error:`, error);
    console.error('[Debug:ErrorBoundary] Component stack:', errorInfo.componentStack);
  }

  render() {
    if (this.state.hasError) {
      return (
        <div
          style={{
            padding: 16,
            margin: 8,
            background: 'rgba(220, 38, 38, 0.1)',
            border: '1px solid rgba(220, 38, 38, 0.3)',
            borderRadius: 8,
            color: '#fca5a5',
            fontSize: 13,
            fontFamily: 'monospace',
          }}
        >
          <div style={{ fontWeight: 600, marginBottom: 8 }}>
            ⚠️ Render Error — {this.props.label || 'Unknown component'}
          </div>
          <div style={{ fontSize: 12, opacity: 0.8 }}>
            {this.state.error?.message || 'Unknown error'}
          </div>
        </div>
      );
    }
    return this.props.children;
  }
}
