import { describe, expect, it } from 'vitest';
import { formatCents, parseCentsInput } from './money';

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

describe('parseCentsInput', () => {
	it('parses a whole dollar amount', () => {
		expect(parseCentsInput('100')).toBe(10000);
	});

	it('parses dollars and cents', () => {
		expect(parseCentsInput('10.00')).toBe(1000);
	});

	it('parses a single cent', () => {
		expect(parseCentsInput('0.01')).toBe(1);
	});

	it('parses a value with one fractional digit', () => {
		expect(parseCentsInput('1.1')).toBe(110);
	});

	it('truncates fractional digits beyond two (the float trap)', () => {
		expect(parseCentsInput('1.005')).toBe(100);
	});

	it('returns 0 for empty string', () => {
		expect(parseCentsInput('')).toBe(0);
	});

	it('returns 0 for whitespace', () => {
		expect(parseCentsInput('   ')).toBe(0);
	});

	it('returns 0 for non-numeric input', () => {
		expect(parseCentsInput('abc')).toBe(0);
	});

	it('parses negative amounts', () => {
		expect(parseCentsInput('-10.00')).toBe(-1000);
	});
});
