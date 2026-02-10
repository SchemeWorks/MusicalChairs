/**
 * Formats ICP values by limiting to 8 decimal places and removing unnecessary trailing zeros
 * Examples:
 * - 500.00 -> "500"
 * - 23.4560000 -> "23.456"
 * - 0.1000 -> "0.1"
 * - 123.456789123456 -> "123.45678912" (limited to 8 decimals)
 * - 0.00000001 -> "0.00000001" (preserves up to 8 decimals)
 */
export function formatICP(value: number): string {
  // Handle edge cases
  if (isNaN(value) || !isFinite(value)) {
    return "0";
  }

  // Limit to 8 decimal places first
  const limitedValue = Math.round(value * 100000000) / 100000000;
  
  // Convert to string with fixed 8 decimal places, then remove trailing zeros
  const str = limitedValue.toFixed(8);
  
  // Remove trailing zeros after decimal point, and the decimal point if no fractional part remains
  const trimmed = str.replace(/\.?0+$/, '');
  
  return trimmed;
}

/**
 * Formats ICP values with the "ICP" suffix, limited to 8 decimal places
 */
export function formatICPWithSuffix(value: number): string {
  return `${formatICP(value)} ICP`;
}

/**
 * Validates that a number has no more than 8 decimal places
 */
export function validateEightDecimals(value: number): boolean {
  if (isNaN(value) || !isFinite(value)) {
    return false;
  }
  
  // Convert to string and check decimal places
  const str = value.toString();
  const decimalIndex = str.indexOf('.');
  
  if (decimalIndex === -1) {
    return true; // No decimal places
  }
  
  const decimalPlaces = str.length - decimalIndex - 1;
  return decimalPlaces <= 8;
}

/**
 * Validates that a string input represents a valid number with no more than 8 decimal places
 */
export function validateICPInput(input: string): { isValid: boolean; error?: string } {
  if (!input || input.trim() === '') {
    return { isValid: true }; // Empty input is valid (will be handled by required validation)
  }

  const value = parseFloat(input);
  
  if (isNaN(value)) {
    return { isValid: false, error: 'Please enter a valid number' };
  }

  if (!validateEightDecimals(value)) {
    return { isValid: false, error: 'Maximum 8 decimal places allowed' };
  }

  return { isValid: true };
}

/**
 * Restricts input to maximum 8 decimal places by truncating excess digits
 */
export function restrictToEightDecimals(input: string): string {
  if (!input || input.trim() === '') {
    return input;
  }

  const decimalIndex = input.indexOf('.');
  if (decimalIndex === -1) {
    return input; // No decimal point
  }

  const beforeDecimal = input.substring(0, decimalIndex + 1);
  const afterDecimal = input.substring(decimalIndex + 1);
  
  // Limit to 8 decimal places
  const limitedAfterDecimal = afterDecimal.substring(0, 8);
  
  return beforeDecimal + limitedAfterDecimal;
}
