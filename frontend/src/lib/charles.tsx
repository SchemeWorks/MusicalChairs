/**
 * Charles — The shadowy figure behind Musical Chairs.
 * Named after Charles Ponzi (1882-1949), the original scheme artist.
 *
 * Visual identity based on the real Ponzi: slicked-back hair, prominent
 * mustache, boater hat, walking cane, three-piece suit. A 5'2" Italian
 * immigrant who convinced Boston he'd found a money glitch.
 */

// The three principals with access to Charles's Office
export const CHARLES_PRINCIPALS: string[] = [
  'zs6vm-4yyag-sbw7x-6ipms-h4tmz-ox4pu-mcq3b-thtt4-de25x-wmsh4-rqe',
  'stzp3-bnvwm-zqzjh-o6mv6-ci53m-wj5k6-xyhe7-fnyp2-c64o3-7vokj-bqe',
  'zegjz-jpi6k-qkand-c2bgf-qw6za-xk4si-nz3gx-qzzia-fk6fg-snepb-tae',
];

/**
 * Check if a principal belongs to Charles.
 * Used for frontend admin gating — the backend enforces permissions independently.
 */
export function isCharles(principal: string): boolean {
  return CHARLES_PRINCIPALS.includes(principal);
}

/**
 * Charles's silhouette SVG — boater hat, mustache, cane.
 * Based on actual photographs of Charles Ponzi circa 1920.
 *
 * Usage: <CharlesIcon className="h-4 w-4" />
 */
export function CharlesIcon({ className = 'h-4 w-4' }: { className?: string }) {
  return (
    <svg
      viewBox="0 0 64 64"
      fill="currentColor"
      className={className}
      aria-label="Charles"
    >
      {/* Boater hat — flat-topped, wide brim */}
      <rect x="12" y="10" width="40" height="5" rx="1" />
      <rect x="18" y="4" width="28" height="8" rx="2" />

      {/* Head */}
      <ellipse cx="32" cy="22" rx="10" ry="9" />

      {/* Mustache — the signature */}
      <path d="M24 26 Q28 30 32 27 Q36 30 40 26 Q38 29 32 29 Q26 29 24 26Z" />

      {/* Body — three-piece suit silhouette */}
      <path d="M22 31 L18 52 L26 52 L28 38 L32 42 L36 38 L38 52 L46 52 L42 31 Q37 28 32 28 Q27 28 22 31Z" />

      {/* Walking cane — diagonal, right side */}
      <line x1="44" y1="32" x2="52" y2="58" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" />
      <circle cx="44" cy="31" r="2" />

      {/* Lapel detail — boutonniere hint */}
      <circle cx="30" cy="34" r="1.5" opacity="0.6" />
    </svg>
  );
}
