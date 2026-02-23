/**
 * Parse an address string as hex (0x-prefixed or plain hex) falling back to decimal.
 * Replaces the repeated `parseInt(s, 16) || parseInt(s)` pattern.
 */
export function parseAddress(s: string): number {
    return parseInt(s, 16) || parseInt(s, 10);
}
