export function Footer() {
  return (
    <footer className="mc-border-subtle border-t mt-8 pt-3 pb-4 px-4 text-xs mc-text-muted">
      <div className="max-w-4xl mx-auto flex items-center justify-between gap-3">
        <span>© 2026 Musical Chairs</span>
        <a
          href="https://internetcomputer.org"
          target="_blank"
          rel="noreferrer"
          className="shrink-0 opacity-80 hover:opacity-100 transition-opacity"
        >
          <img src="/built-on-icp.png" alt="Built on the Internet Computer" className="h-5" />
        </a>
        <button
          type="button"
          className="hover:mc-text-primary"
          onClick={() => window.dispatchEvent(new CustomEvent('mc:open-docs'))}
        >
          Docs
        </button>
      </div>
    </footer>
  );
}
