/**
 * Interpolate a description/backfire string against a spell's runtime
 * numbers. Used so admin tunes to `effectValues` / `duration` flow into
 * the displayed copy without anyone editing the description text.
 *
 * Supported placeholders:
 *   {0}, {1}, {2}, ...  - effectValues[N]. Rendered as the numeric value
 *                          with up to 1 decimal trimmed (e.g. 5 → "5",
 *                          1.3 → "1.3", 250 → "250"). Out-of-range index
 *                          leaves the placeholder as-is.
 *   {dur_h}              - duration in hours, as a whole-number string.
 *   {dur_d}              - duration converted to whole days
 *                          (Math.round(durationHours / 24)).
 *
 * No placeholders → input returned unchanged (so legacy non-templated
 * descriptions keep rendering).
 */
export function renderTemplate(
  text: string,
  effectValues: number[],
  durationHours: number,
): string {
  if (!text || text.indexOf('{') === -1) return text;
  return text.replace(/\{([^}]+)\}/g, (full, key) => {
    if (key === 'dur_h') return String(Math.round(durationHours));
    if (key === 'dur_d') return String(Math.round(durationHours / 24));
    const idx = Number(key);
    if (Number.isInteger(idx) && idx >= 0 && idx < effectValues.length) {
      const v = effectValues[idx];
      // Trim trailing zero on decimals (5.0 → "5", 1.3 → "1.3").
      return Number.isInteger(v) ? String(v) : String(parseFloat(v.toFixed(2)));
    }
    return full; // leave unknown placeholders alone
  });
}
