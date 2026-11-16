/**
 * An amount below the smallest displayable unit reads "<$0.01", so a real but tiny spend is
 * never drawn as the true zero "$0.00".
 */
export function money(value: number, currency: string): string {
  let formatter: Intl.NumberFormat | null = null;
  try {
    formatter = new Intl.NumberFormat("en-US", { style: "currency", currency });
  } catch {
    formatter = null;
  }
  const digits = formatter?.resolvedOptions().maximumFractionDigits ?? 2;
  const show = (amount: number) => (formatter ? formatter.format(amount) : `${amount.toFixed(digits)} ${currency}`);
  const smallest = 1 / 10 ** digits;
  return value > 0 && value < smallest ? `<${show(smallest)}` : show(value);
}

/**
 * The pill's headline when a summary arrived but holds no cost to show. No session started is a true
 * "nothing recorded yet", never a zero. Sessions that started with no reported cost read "no cost
 * reported", so a missing cost is not drawn as $0. The popover keeps the full reason.
 */
export function unpricedHeadline(costCoverage: { sessions: number } | undefined): string {
  if (costCoverage === undefined) return "not available";
  return costCoverage.sessions === 0 ? "nothing recorded yet" : "no cost reported";
}
