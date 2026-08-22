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
