import { describe, expect, it } from 'vitest';
import { formatCueList, formatTime, listFound, parseCueList, remainingPercent } from './qlab';

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

describe('listFound', () => {
  const show = { host: '10.0.0.7', name: 'Show', id: 'A', version: '5.6.5' };
  const backup = { ...show, host: '10.0.0.8' };

  it('keeps a workspace missing from the latest scan', () => {
    expect(listFound([show], [])).toEqual([show]);
  });

  it('forgets one missing from both scans', () => {
    expect(listFound([], [])).toEqual([]);
  });

  it('lists the same workspace once per computer, newest details first', () => {
    const renamed = { ...show, name: 'Show v2' };
    expect(listFound([show, backup], [renamed])).toEqual([renamed, backup]);
  });
});
