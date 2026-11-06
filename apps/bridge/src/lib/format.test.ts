import { describe, expect, it } from 'vitest';
import { actionLabel } from './format';

describe('actionLabel', () => {
  it('labels each key action', () => {
    expect(actionLabel('next')).toBe('Next');
    expect(actionLabel('prev')).toBe('Prev');
  });
});
