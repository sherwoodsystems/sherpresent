import type { CaptionApiKeyProviderId, CaptionProviderId, CaptionsConfig } from './types';

/** Spoken / caption languages offered in Settings, as BCP-47 tags. Any pair
 * works (en/fr/es in either direction). A bare language is fine as a source:
 * the Apple helper settles it on the likely region (fr -> fr-FR). */
export const CAPTION_LANGUAGES: { code: string; label: string }[] = [
  { code: 'en-US', label: 'English' },
  { code: 'fr', label: 'French' },
  { code: 'es', label: 'Spanish' }
];

/** Mirrors Rust's `provider::same_language`: compare primary subtags only. */
export function sameLanguage(a: string, b: string): boolean {
  const primary = (tag: string) => tag.trim().split(/[-_]/)[0].toLowerCase();
  const pa = primary(a);
  return pa !== '' && pa === primary(b);
}

/** Mirrors Rust's `CaptionsConfig::translates`: same language = straight captions. */
export function translates(c: Pick<CaptionsConfig, 'sourceLanguage' | 'targetLanguage'>): boolean {
  return c.sourceLanguage === null || !sameLanguage(c.sourceLanguage, c.targetLanguage);
}

/** Which API key a provider needs, or null for on-device providers. */
export function apiKeyProvider(provider: CaptionProviderId): CaptionApiKeyProviderId | null {
  return provider === 'apple' ? null : provider;
}

export const API_KEY_LABELS: Record<CaptionApiKeyProviderId, string> = {
  gemini: 'Gemini',
  openai: 'OpenAI'
};
