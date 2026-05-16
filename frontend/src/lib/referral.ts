const REF_KEY = 'mc_referrer';

// Read `?ref=<code-or-principal>` from the URL once on app load and stash it
// in localStorage so it survives the auth round-trip. The token can be either
// a short 6-char referral code (current format) or a full principal (legacy
// links). First referrer wins on the frontend AND on the canister.
export function captureReferrerFromUrl(): void {
  if (typeof window === 'undefined') return;
  try {
    const params = new URLSearchParams(window.location.search);
    const ref = params.get('ref');
    if (!ref) return;
    if (localStorage.getItem(REF_KEY)) return;
    if (!/^[A-Za-z0-9-]{1,128}$/.test(ref)) return;
    localStorage.setItem(REF_KEY, ref);
  } catch {
    // localStorage unavailable (private mode quota etc.) — ignore
  }
}

export function getStoredReferrer(): string | null {
  try {
    return localStorage.getItem(REF_KEY);
  } catch {
    return null;
  }
}

export function buildReferralLink(token: string | null | undefined): string {
  if (!token) return 'https://musicalchairs.fun/';
  return `https://musicalchairs.fun/?ref=${token}`;
}
