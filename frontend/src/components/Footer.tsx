import { ExternalLink } from 'lucide-react';

export function Footer() {
  return (
    <footer className="mc-border-subtle border-t mt-12 pt-6 pb-8 px-4 text-center text-xs mc-text-muted">
      <div className="max-w-4xl mx-auto flex flex-col gap-3 md:flex-row md:justify-between md:items-center">
        <div className="flex items-center justify-center gap-2">
          <span>© 2026 Musical Chairs</span>
          <span aria-hidden>·</span>
          <span>Built on the Internet Computer</span>
        </div>
        <div className="flex items-center justify-center gap-4">
          <a
            href="https://internetcomputer.org"
            target="_blank"
            rel="noreferrer"
            className="inline-flex items-center gap-1 hover:mc-text-primary"
          >
            ICP <ExternalLink className="w-3 h-3" />
          </a>
          <button type="button" className="hover:mc-text-primary" onClick={() => {
            window.dispatchEvent(new CustomEvent('mc:open-docs'));
          }}>
            Docs
          </button>
          <span className="opacity-60">Not financial advice. For entertainment only.</span>
        </div>
      </div>
    </footer>
  );
}
