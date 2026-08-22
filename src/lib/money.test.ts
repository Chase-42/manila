import { describe, expect, it } from 'vitest';
import { formatCents } from './money';

describe('formatCents', () => {
	it('formats positive cents', () => {
		expect(formatCents(5000)).toBe('$50.00');
	});

	it('formats negative cents', () => {
		expect(formatCents(-5000)).toBe('-$50.00');
	});

	it('formats zero', () => {
		expect(formatCents(0)).toBe('$0.00');
	});

	it('preserves cents precision', () => {
		expect(formatCents(199)).toBe('$1.99');
	});
});
