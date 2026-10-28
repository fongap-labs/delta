/**
 * The composer's model after the settings are read again. A model the user picked for this session
 * is kept. The composer follows the settings default only while it has no model yet, or while it
 * still shows the previous default, so a default set on the Models page reaches the composer at once.
 */
export function modelAfterSettings(current: string, previousDefault: string, nextDefault: string): string {
  if (!nextDefault) return current;
  if (!current || current === previousDefault) return nextDefault;
  return current;
}
