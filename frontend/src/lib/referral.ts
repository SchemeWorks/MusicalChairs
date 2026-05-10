import { Principal } from '@dfinity/principal';

const REF_KEY = 'mc_referrer';

// Read `?ref=<principal>` from the URL once on app load and stash it in
// localStorage so it survives the auth round-trip. First referrer wins (we
// only set if nothing's stored yet); the backend also enforces this rule
// inside registerReferral.
export function captureReferrerFromUrl(): void {
  if (typeof window === 'undefined') return;
  try {
    const params = new URLSearchParams(window.location.search);
    const ref = params.get('ref');
    if (!ref) return;
    if (localStorage.getItem(REF_KEY)) return;
    Principal.fromText(ref); // validates it parses
    localStorage.setItem(REF_KEY, ref);
  } catch {
    // Malformed principal — ignore so the user can still use the app
  }
}

export function getStoredReferrer(): string | null {
  try {
    return localStorage.getItem(REF_KEY);
  } catch {
    return null;
  }
}

export function buildReferralLink(principal: string | null | undefined): string {
  if (!principal) return 'https://musicalchairs.fun/';
  return `https://musicalchairs.fun/?ref=${principal}`;
}
