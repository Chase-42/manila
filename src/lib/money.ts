const FORMATTERS = new Map<string, Intl.NumberFormat>();

function getFormatter(currency: string): Intl.NumberFormat {
	let fmt = FORMATTERS.get(currency);
	if (!fmt) {
		fmt = new Intl.NumberFormat('en-US', { style: 'currency', currency });
		FORMATTERS.set(currency, fmt);
	}
	return fmt;
}

/** Format an integer cents value as a display string. 5000 -> "$50.00", -5000 -> "-$50.00" */
export function formatCents(cents: number, currency = 'USD'): string {
	return getFormatter(currency).format(cents / 100);
}

/** Parse a user-entered decimal string to integer cents with no float arithmetic.
 *  Fractional digits beyond two are truncated, not rounded. Returns 0 for empty or unparseable input. */
export function parseCentsInput(val: string): number {
	const trimmed = val.trim();
	if (!trimmed) return 0;
	const negative = trimmed.startsWith('-');
	const abs = negative ? trimmed.slice(1) : trimmed;
	const dotIdx = abs.indexOf('.');
	if (dotIdx === -1) {
		const n = parseInt(abs, 10);
		return isNaN(n) ? 0 : (negative ? -n : n) * 100;
	}
	const intStr = abs.slice(0, dotIdx) || '0';
	const fracStr = abs.slice(dotIdx + 1).padEnd(2, '0').slice(0, 2);
	const intVal = parseInt(intStr, 10);
	const fracVal = parseInt(fracStr, 10);
	if (isNaN(intVal) || isNaN(fracVal)) return 0;
	const result = intVal * 100 + fracVal;
	return negative ? -result : result;
}
