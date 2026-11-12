/** Source-level refusal includes historical fixture commit IDs without a colon.
 * The shared DaemonApi guard is outside this module's ownership.
 */
export function isFixtureLikeId(id: string): boolean {
  return id.trim().toLowerCase().startsWith("fixture");
}
