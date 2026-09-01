/**
 * Normalizes a numeric ID from a route query (or param) value.
 *
 * vue-router types query values as `LocationQueryValue | LocationQueryValue[]`,
 * so a malformed link like `?contestId=7&contestId=8` yields an array and a
 * naive `Number(route.query.contestId)` produces NaN. This returns the first
 * value as a positive safe integer, or `null` for anything else.
 */
export function numericQueryId(value: unknown): number | null {
  const raw = Array.isArray(value) ? value[0] : value;
  const id = Number(raw);
  return Number.isSafeInteger(id) && id > 0 ? id : null;
}
