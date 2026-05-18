const REF_KEY = 'mc_referrer';

// Read `?ref=<code-or-principal>` from the URL on app load and stash it in
// localStorage so it survives the auth round-trip. The token can be either a
// short 6-char referral code (current format) or a full principal (legacy
// links). A fresh `?ref=` in the URL always overwrites any stored value —
// clicking a referral link is a stronger signal of intent than whatever was
// stashed on a prior visit. First-wins is enforced authoritatively by
// shenanigans.registerReferral on the canister; once a downliner's chain
// entry is set, subsequent registerReferral calls are no-ops, so overwriting
// the frontend cache can't corrupt an existing chain.
export function captureReferrerFromUrl(): void {
  if (typeof window === 'undefined') return;
  try {
    const params = new URLSearchParams(window.location.search);
    const ref = params.get('ref');
    if (!ref) return;
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
