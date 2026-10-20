import { describe, expect, it } from 'vitest';
import { formatCueList, formatTime, parseCueList, remainingPercent } from './qlab';

// Same cases as QLabContentTests.swift: the tab, the page and Syphon agree.
describe('formatTime', () => {
  it('rounds up so 0:00 only shows at the end', () => {
    expect(formatTime(9.2)).toBe('0:10');
    expect(formatTime(10)).toBe('0:10');
    expect(formatTime(0.05)).toBe('0:01');
    expect(formatTime(0)).toBe('0:00');
    expect(formatTime(-3)).toBe('0:00');
    expect(formatTime(75)).toBe('1:15');
    expect(formatTime(3725)).toBe('1:02:05');
  });
});

describe('remainingPercent', () => {
  const cue = { number: '1', name: '', paused: false, others: 0 };
  it('is the share left, clamped', () => {
    expect(remainingPercent({ ...cue, remaining: 30, duration: 120 })).toBe(25);
    expect(remainingPercent({ ...cue, remaining: 200, duration: 120 })).toBe(100);
    expect(remainingPercent({ ...cue, remaining: 0, duration: 0 })).toBe(0);
  });
});

describe('cue list', () => {
  it('splits on commas and spaces, dropping blanks and repeats', () => {
    expect(parseCueList(' 12, 14  3.5,,12 ')).toEqual(['12', '14', '3.5']);
    expect(parseCueList('')).toEqual([]);
    expect(formatCueList(['12', '14'])).toBe('12, 14');
  });
});
