import React, { Component, ReactNode } from 'react';
import { Zap } from 'lucide-react';

interface Props {
  children: ReactNode;
  fallback?: ReactNode;
}

interface State {
  hasError: boolean;
  error: Error | null;
}

const errorQuips = [
  "The house always wins. Except right now.",
  "Even Ponzis have bad days.",
  "Charles is looking into it. He's not, but it sounds reassuring.",
  "Something broke. Probably not the math. Probably.",
];

export default class ErrorBoundary extends Component<Props, State> {
  state: State = { hasError: false, error: null };

  static getDerivedStateFromError(error: Error): State {
    return { hasError: true, error };
  }

  componentDidCatch(error: Error, info: React.ErrorInfo) {
    console.error('ErrorBoundary caught:', error, info);
  }

  render() {
    if (this.state.hasError) {
      if (this.props.fallback) return this.props.fallback;
      const quip = errorQuips[Math.floor(Math.random() * errorQuips.length)];
      return (
        <div className="mc-status-red p-6 text-center m-4 rounded-xl">
          <Zap className="h-8 w-8 mc-text-danger mb-3 mx-auto" />
          <p className="font-accent text-sm mc-text-primary mb-2">{quip}</p>
          <p className="text-xs mc-text-dim mb-4">{this.state.error?.message || 'Unknown error'}</p>
          <button
            onClick={() => this.setState({ hasError: false, error: null })}
            className="mc-btn-secondary px-4 py-2 text-xs rounded-lg"
          >
            Spin Again
          </button>
        </div>
      );
    }
    return this.props.children;
  }
}
