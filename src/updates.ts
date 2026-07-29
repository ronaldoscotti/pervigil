/** How often to look for a new release while the app keeps running. */
export const UPDATE_CHECK_MS = 6 * 60 * 60 * 1000;

/** A hidden tray window has its timers throttled, so the panel also checks when it
 *  is shown again. This floor keeps that from hitting the endpoint on every open. */
export const UPDATE_FLOOR_MS = 60 * 60 * 1000;

/** Never re-checks once an update is waiting to be installed: the affordance is
 *  already on screen and a newer answer would not change it. */
export function dueForCheck(now: number, lastCheck: number, updatePending: boolean): boolean {
  if (updatePending) return false;
  if (lastCheck === 0) return true;
  return now - lastCheck >= UPDATE_FLOOR_MS;
}
