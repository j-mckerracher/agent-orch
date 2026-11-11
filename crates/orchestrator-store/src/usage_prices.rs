//! Native, versioned price table for ESTIMATED usage cost (board item M7-26).
//!
//! Some providers report tokens but no cost (Codex always). `usage.summary`
//! can turn those recorded token counts into an estimate at summary time. The
//! estimate is never written back: stored receipts stay exactly as the provider
//! reported them, and an estimate is always summarised as `estimated`.
//!
//! # Rules
//!
//! * Prices are USD, integer nano-USD per token, so every figure is exact
//!   integer arithmetic (`$1.25 / 1M tokens` is `1250`).
//! * A figure is `None` (unknown) unless the model is in this table and every
//!   token count the price needs is known. A missing count is never read as 0,
//!   and a model that is not listed is never priced at 0.
//! * Prices are the standard-tier, short-context list prices. Batch, flex,
//!   priority, long-context (above 272k tokens) and 1-hour Anthropic cache
//!   writes are not modelled, because the receipts do not record the tier, the
//!   request size or the cache lifetime. An estimate is therefore a list-price
//!   figure, not a bill, and can sit below the real charge.
//! * No dependency and no network: the table is this constant.
//!
//! # Updating
//!
//! Edit [`PRICES`] and bump [`PRICE_TABLE_VERSION`] in the same change; the
//! version is shown next to every estimate. Source of the current rows: the
//! LiteLLM `model_prices_and_context_window.json` list (retrieved 2026-11-11),
//! transcribed by hand. Cross-checked against a recorded Claude Code run whose
//! CLI-reported cost it reproduces exactly (see the tests).
//!
//! Updated: 2026-11-11T01:30:00+00:00.

/// Bumped whenever [`PRICES`] changes. Shown with every estimate.
pub const PRICE_TABLE_VERSION: &str = "2026-11-11.1";
/// How an estimate is derived. Part of the `usage.summary` contract.
pub const PRICE_TABLE_BASIS: &str = "list_price_standard_tier";
/// Human-readable origin of the rows.
pub const PRICE_TABLE_SOURCE: &str = "LiteLLM model_prices_and_context_window.json, 2026-11-11";
/// The only currency of the table.
pub const PRICE_TABLE_CURRENCY: &str = "USD";

/// One model's list price in nano-USD per token.
struct ModelPrice {
    model: &'static str,
    input: u64,
    cache_read: u64,
    /// `None` when the provider has no cache-write charge.
    cache_write: Option<u64>,
    output: u64,
}

const fn price(
    model: &'static str,
    input: u64,
    cache_read: u64,
    cache_write: Option<u64>,
    output: u64,
) -> ModelPrice {
    ModelPrice {
        model,
        input,
        cache_read,
        cache_write,
        output,
    }
}

/// Canonical ids: lowercase; Claude ids use dashes in version numbers.
const PRICES: &[ModelPrice] = &[
    // OpenAI / Codex.
    price("gpt-6.1-sol", 2_000, 100, Some(2_500), 10_000),
    price("gpt-6.1-terra", 1_000, 50, Some(1_250), 5_000),
    price("gpt-6-sol", 2_000, 200, Some(2_500), 10_000),
    price("gpt-6-luna", 100, 10, Some(125), 500),
    price("gpt-6-astra", 10_000, 1_000, Some(12_500), 50_000),
    price("gpt-5.6-sol", 4_000, 400, Some(5_000), 20_000),
    price("gpt-5.6-terra", 2_000, 200, Some(2_500), 12_000),
    price("gpt-5.6-luna", 200, 20, Some(250), 1_200),
    price("gpt-5.3-codex", 1_750, 175, None, 14_000),
    // Anthropic / Claude Code (5-minute cache-write rate).
    price("claude-opus-5-5", 4_000, 200, Some(5_000), 20_000),
    price("claude-opus-5", 5_000, 500, Some(6_250), 25_000),
    price("claude-opus-4-8", 5_000, 500, Some(6_250), 25_000),
    price("claude-sonnet-5-5", 2_000, 100, Some(2_500), 10_000),
    price("claude-sonnet-5", 2_000, 200, Some(2_500), 10_000),
    price("claude-haiku-5-5", 100, 10, Some(125), 500),
    price("claude-haiku-4-5", 1_000, 100, Some(1_250), 5_000),
];

/// Token counts of one receipt, in the normalised meaning of
/// `orchestrator-agents::usage`: `input` excludes cache reads and writes and
/// `output` already includes reasoning. `None` is unknown.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TokenCounts {
    pub input: Option<u64>,
    pub cache_read: Option<u64>,
    pub cache_write: Option<u64>,
    pub output: Option<u64>,
}

/// What the summary tells clients about the table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PriceTableInfo {
    pub version: &'static str,
    pub basis: &'static str,
    pub source: &'static str,
    pub currency: &'static str,
    pub models: usize,
}

pub fn price_table_info() -> PriceTableInfo {
    PriceTableInfo {
        version: PRICE_TABLE_VERSION,
        basis: PRICE_TABLE_BASIS,
        source: PRICE_TABLE_SOURCE,
        currency: PRICE_TABLE_CURRENCY,
        models: PRICES.len(),
    }
}

/// Maps a provider model string to a table id: case-folded, a trailing
/// `[context]` tag and a trailing `-YYYYMMDD` date dropped, and dots in Claude
/// version numbers turned into dashes (`claude-opus-4.8` is `claude-opus-4-8`).
fn canonical_model(model: &str) -> String {
    let mut id = model.trim().to_ascii_lowercase();
    if let Some(open) = id.find('[') {
        id.truncate(open);
    }
    if let Some((head, tail)) = id.rsplit_once('-') {
        if tail.len() == 8 && tail.bytes().all(|byte| byte.is_ascii_digit()) {
            id = head.to_owned();
        }
    }
    if id.starts_with("claude-") {
        id = id.replace('.', "-");
    }
    id
}

fn lookup(model: &str) -> Option<&'static ModelPrice> {
    let id = canonical_model(model);
    PRICES.iter().find(|entry| entry.model == id)
}

/// Whether the table can price `model`.
pub fn is_priced_model(model: &str) -> bool {
    lookup(model).is_some()
}

/// Estimated cost in nano-USD, or `None` (unknown) when the model is absent or
/// unlisted, or a count the price needs is unknown. A model with no cache-write
/// charge accepts an unknown or zero cache-write count but not a positive one,
/// since that charge is not in the table.
pub fn estimate_cost_nanos(model: Option<&str>, tokens: &TokenCounts) -> Option<u64> {
    let price = lookup(model?)?;
    let cache_write_cost = match (price.cache_write, tokens.cache_write) {
        (Some(unit), Some(count)) => count.checked_mul(unit)?,
        (None, None | Some(0)) => 0,
        _ => return None,
    };
    tokens
        .input?
        .checked_mul(price.input)?
        .checked_add(tokens.cache_read?.checked_mul(price.cache_read)?)?
        .checked_add(cache_write_cost)?
        .checked_add(tokens.output?.checked_mul(price.output)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn counts(input: u64, cache_read: u64, cache_write: u64, output: u64) -> TokenCounts {
        TokenCounts {
            input: Some(input),
            cache_read: Some(cache_read),
            cache_write: Some(cache_write),
            output: Some(output),
        }
    }

    #[test]
    fn every_model_prices_from_its_own_row() {
        for entry in PRICES {
            let got = estimate_cost_nanos(Some(entry.model), &counts(1_000, 2_000, 0, 3_000))
                .unwrap_or_else(|| panic!("{} must be priced", entry.model));
            let want = 1_000 * entry.input + 2_000 * entry.cache_read + 3_000 * entry.output;
            assert_eq!(got, want, "{}", entry.model);
            if let Some(write) = entry.cache_write {
                let with_write =
                    estimate_cost_nanos(Some(entry.model), &counts(1_000, 2_000, 4_000, 3_000))
                        .unwrap();
                assert_eq!(with_write, want + 4_000 * write, "{}", entry.model);
            }
        }
    }

    #[test]
    fn table_rows_are_unique_canonical_and_priced() {
        for (index, entry) in PRICES.iter().enumerate() {
            assert_eq!(canonical_model(entry.model), entry.model);
            assert!(PRICES[..index]
                .iter()
                .all(|other| other.model != entry.model));
            assert!(entry.input > 0 && entry.output > 0 && entry.cache_read > 0);
        }
    }

    #[test]
    fn codex_recorded_turn_matches_a_hand_computation() {
        // M1-05 capture: input 72641 incl. 48128 cached, output 419.
        let tokens = counts(72_641 - 48_128, 48_128, 0, 419);
        // 24513*100 + 48128*10 + 419*500 nano-USD.
        assert_eq!(
            estimate_cost_nanos(Some("gpt-6-luna"), &tokens),
            Some(3_142_080)
        );
        // gpt-5.3-codex has no cache-write charge, so a zero count is fine.
        assert_eq!(
            estimate_cost_nanos(Some("gpt-5.3-codex"), &tokens),
            Some(24_513 * 1_750 + 48_128 * 175 + 419 * 14_000)
        );
    }

    #[test]
    fn reproduces_the_recorded_claude_code_cost_exactly() {
        // M4-02 capture: total_cost_usd 0.0086388 on claude-opus-5-5.
        assert_eq!(
            estimate_cost_nanos(Some("claude-opus-5-5"), &counts(2, 2_254, 1_360, 69)),
            Some(8_638_800)
        );
    }

    #[test]
    fn unknown_or_missing_model_is_unknown_never_zero() {
        let tokens = counts(1, 1, 0, 1);
        assert_eq!(estimate_cost_nanos(None, &tokens), None);
        assert_eq!(estimate_cost_nanos(Some(""), &tokens), None);
        assert_eq!(estimate_cost_nanos(Some("gpt-9-imaginary"), &tokens), None);
        assert_eq!(estimate_cost_nanos(Some("gpt-6"), &tokens), None);
        assert!(!is_priced_model("local-llama"));
    }

    #[test]
    fn a_missing_token_count_is_unknown_never_zero() {
        let full = counts(10, 20, 30, 40);
        assert!(estimate_cost_nanos(Some("claude-opus-5"), &full).is_some());
        for broken in [
            TokenCounts {
                input: None,
                ..full
            },
            TokenCounts {
                cache_read: None,
                ..full
            },
            TokenCounts {
                cache_write: None,
                ..full
            },
            TokenCounts {
                output: None,
                ..full
            },
        ] {
            assert_eq!(estimate_cost_nanos(Some("claude-opus-5"), &broken), None);
        }
        assert_eq!(
            estimate_cost_nanos(Some("claude-opus-5"), &TokenCounts::default()),
            None
        );
    }

    #[test]
    fn no_cache_write_price_accepts_unknown_or_zero_but_not_positive_counts() {
        let base = counts(10, 20, 0, 40);
        let model = Some("gpt-5.3-codex");
        assert!(estimate_cost_nanos(model, &base).is_some());
        assert_eq!(
            estimate_cost_nanos(
                model,
                &TokenCounts {
                    cache_write: None,
                    ..base
                }
            ),
            estimate_cost_nanos(model, &base)
        );
        assert_eq!(
            estimate_cost_nanos(
                model,
                &TokenCounts {
                    cache_write: Some(1),
                    ..base
                }
            ),
            None
        );
    }

    #[test]
    fn model_names_are_normalised_before_lookup() {
        let tokens = counts(1, 1, 1, 1);
        let want = estimate_cost_nanos(Some("claude-opus-4-8"), &tokens);
        assert!(want.is_some());
        for alias in [
            "claude-opus-4.8",
            "CLAUDE-OPUS-4-8",
            "claude-opus-4-8-20260101",
            "claude-opus-4-8[1m]",
            " claude-opus-4.8 ",
        ] {
            assert_eq!(estimate_cost_nanos(Some(alias), &tokens), want, "{alias}");
        }
        // Dots are significant in OpenAI ids.
        assert!(is_priced_model("gpt-6.1-sol"));
        assert!(!is_priced_model("gpt-6-1-sol"));
    }

    #[test]
    fn overflow_is_unknown_not_a_wrapped_figure() {
        assert_eq!(
            estimate_cost_nanos(Some("gpt-6-astra"), &counts(u64::MAX, 0, 0, 0)),
            None
        );
    }

    #[test]
    fn info_names_the_version_basis_and_size() {
        let info = price_table_info();
        assert_eq!(info.version, PRICE_TABLE_VERSION);
        assert_eq!(info.basis, "list_price_standard_tier");
        assert_eq!(info.currency, "USD");
        assert_eq!(info.models, PRICES.len());
    }
}
