import { describe, expect, it } from 'vitest';
import {
  API_KEY_LABELS,
  CAPTION_LANGUAGES,
  apiKeyProvider,
  sameLanguage,
  translates
} from './captions';

// These mirror Rust (`provider::same_language`, `CaptionsConfig::translates`),
// so the cases match the Rust tests: Settings and the engine must agree on
// whether a session translates.
describe('sameLanguage', () => {
  it('ignores region and case', () => {
    expect(sameLanguage('en-US', 'en')).toBe(true);
    expect(sameLanguage('EN', 'en-GB')).toBe(true);
    expect(sameLanguage('fr', 'fr_CA')).toBe(true);
  });

  it('distinguishes languages', () => {
    expect(sameLanguage('en', 'fr')).toBe(false);
  });

  it('never matches empty tags', () => {
    expect(sameLanguage('', '')).toBe(false);
    expect(sameLanguage('  ', 'en')).toBe(false);
  });
});

describe('translates', () => {
  it('is true for a real pair', () => {
    expect(translates({ sourceLanguage: 'en-US', targetLanguage: 'fr' })).toBe(true);
  });

  it('is false for straight captions', () => {
    expect(translates({ sourceLanguage: 'en-US', targetLanguage: 'en' })).toBe(false);
  });

  it('is true for auto-detect, which may still translate', () => {
    expect(translates({ sourceLanguage: null, targetLanguage: 'en' })).toBe(true);
  });
});

describe('apiKeyProvider', () => {
  it('needs no key on-device', () => {
    expect(apiKeyProvider('apple')).toBeNull();
  });

  it('maps cloud providers to their key and a label', () => {
    for (const p of ['gemini', 'openai'] as const) {
      expect(apiKeyProvider(p)).toBe(p);
      expect(API_KEY_LABELS[p]).toBeTruthy();
    }
  });
});

describe('CAPTION_LANGUAGES', () => {
  it('has unique primary languages, so "Captions In" never offers the source twice', () => {
    const primaries = CAPTION_LANGUAGES.map((l) => l.code.split('-')[0]);
    expect(new Set(primaries).size).toBe(primaries.length);
  });
});
