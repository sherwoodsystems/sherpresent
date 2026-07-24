// Small shared formatting helpers for the bridge UI.

import type { KeyAction } from './types';

/** Human-readable label for a key action ("Next" / "Prev"). */
export function actionLabel(action: KeyAction): string {
  return action === 'next' ? 'Next' : 'Prev';
}
