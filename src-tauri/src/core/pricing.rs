use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::event::Timestamp;

/// Billable token classes. Cache writes are split by TTL — the 1h rate is 1.6x the
/// 5m rate, and long sessions write almost entirely 1h, so collapsing them
/// materially understates cost.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenUsage {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write_5m: u64,
    pub cache_write_1h: u64,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct ModelRate {
    pub input: f64,
    pub output: f64,
    pub cache_read: f64,
    pub cache_write_5m: f64,
    pub cache_write_1h: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PriceTable {
    pub per_tokens: f64,
    pub models: HashMap<String, ModelRate>,
}

/// One priced moment: what a model spent, and when.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageEntry {
    pub at: Timestamp,
    pub model: String,
    pub usage: TokenUsage,
}

/// The table shipped with the binary. Parsed at first use; a malformed file is a
/// test failure, never a runtime one.
pub fn shipped() -> PriceTable {
    serde_json::from_str(include_str!("../../../assets/pricing.json"))
        .expect("shipped price table should parse")
}

/// `None` for a model the table doesn't price — the panel shows `—` rather than a
/// confidently wrong figure.
pub fn cost(table: &PriceTable, model: &str, usage: &TokenUsage) -> Option<f64> {
    let rate = table.models.get(model)?;
    let billed = usage.input as f64 * rate.input
        + usage.output as f64 * rate.output
        + usage.cache_read as f64 * rate.cache_read
        + usage.cache_write_5m as f64 * rate.cache_write_5m
        + usage.cache_write_1h as f64 * rate.cache_write_1h;

    Some(billed / table.per_tokens)
}

/// Total cost of entries falling inside `[from, to]`. Entries the table can't price
/// are skipped, so an unknown model never inflates the total.
pub fn cost_in_window<'a>(
    table: &PriceTable,
    entries: impl IntoIterator<Item = &'a UsageEntry>,
    from: Timestamp,
    to: Timestamp,
) -> f64 {
    entries
        .into_iter()
        .filter(|entry| entry.at >= from && entry.at <= to)
        .filter_map(|entry| cost(table, &entry.model, &entry.usage))
        .sum()
}

/// A session's own total. `None` when nothing here can be priced — the row shows
/// `—` rather than `$0.00`, which would read as "this session was free".
pub fn total(table: &PriceTable, entries: &[UsageEntry]) -> Option<f64> {
    let priced: Vec<f64> = entries
        .iter()
        .filter_map(|entry| cost(table, &entry.model, &entry.usage))
        .collect();

    (!priced.is_empty()).then(|| priced.iter().sum())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A real Opus 4.8 turn sampled from a live transcript: almost all spend is the
    /// 1h cache write.
    fn real_turn() -> TokenUsage {
        TokenUsage {
            input: 2,
            output: 343,
            cache_read: 20_458,
            cache_write_5m: 0,
            cache_write_1h: 27_484,
        }
    }

    fn near(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn prices_a_real_turn_to_the_cent() {
        let table = shipped();

        let total = cost(&table, "claude-opus-4-8", &real_turn()).unwrap();

        assert!(near(total, 0.293_654), "got {total}");
    }

    /// The tray reports today while the panel may be showing four hours. Both totals
    /// come from the same entries, and only the window tells them apart.
    #[test]
    fn a_window_reaching_back_to_midnight_counts_what_a_short_one_misses() {
        let table = shipped();
        let midnight = 1_000_000;
        let now = midnight + 15 * 3_600;
        let entry = |at| UsageEntry {
            at,
            model: "claude-opus-4-8".into(),
            usage: real_turn(),
        };
        let entries = [entry(midnight + 60), entry(now - 60)];

        let today = cost_in_window(&table, entries.iter(), midnight, now);
        let last_four_hours = cost_in_window(&table, entries.iter(), now - 4 * 3_600, now);

        assert!(
            today > last_four_hours,
            "this morning is today's spend even when the panel cannot see it"
        );
        assert!(last_four_hours > 0.0, "and the short window still counts");
    }

    #[test]
    fn an_unknown_model_has_no_price() {
        let table = shipped();

        assert_eq!(cost(&table, "some-future-model", &real_turn()), None);
    }

    #[test]
    fn the_one_hour_cache_write_costs_more_than_the_five_minute_one() {
        let table = shipped();
        let write = |usage| cost(&table, "claude-opus-4-8", &usage).unwrap();

        let short = write(TokenUsage {
            cache_write_5m: 1_000_000,
            ..Default::default()
        });
        let long = write(TokenUsage {
            cache_write_1h: 1_000_000,
            ..Default::default()
        });

        assert!(near(short, 6.25), "got {short}");
        assert!(near(long, 10.0), "got {long}");
    }

    #[test]
    fn no_usage_costs_nothing() {
        let table = shipped();

        assert_eq!(
            cost(&table, "claude-opus-4-8", &TokenUsage::default()),
            Some(0.0)
        );
    }

    #[test]
    fn the_shipped_table_prices_the_models_we_expect() {
        let table = shipped();

        for model in [
            "claude-opus-4-8",
            "claude-sonnet-5",
            "claude-haiku-4-5",
            "claude-fable-5",
        ] {
            assert!(table.models.contains_key(model), "missing {model}");
        }
    }

    #[test]
    fn a_window_sums_only_the_entries_inside_it() {
        let table = shipped();
        let entry = |at| UsageEntry {
            at,
            model: "claude-opus-4-8".into(),
            usage: TokenUsage {
                output: 1_000_000,
                ..Default::default()
            },
        };
        let entries = vec![entry(100), entry(200), entry(900)];

        let total = cost_in_window(&table, &entries, 150, 800);

        assert!(near(total, 25.0), "got {total}");
    }

    #[test]
    fn an_unpriced_entry_is_skipped_rather_than_counted_as_zero_silently() {
        let table = shipped();
        let entries = vec![UsageEntry {
            at: 100,
            model: "some-future-model".into(),
            usage: real_turn(),
        }];

        assert!(near(cost_in_window(&table, &entries, 0, 200), 0.0));
    }

    #[test]
    fn a_session_nothing_can_price_has_no_total() {
        let table = shipped();
        let entries = vec![UsageEntry {
            at: 100,
            model: "some-future-model".into(),
            usage: real_turn(),
        }];

        assert_eq!(total(&table, &entries), None);
    }

    #[test]
    fn a_session_with_no_usage_at_all_has_no_total() {
        assert_eq!(total(&shipped(), &[]), None);
    }

    #[test]
    fn a_priceable_session_totals_every_entry() {
        let table = shipped();
        let entries = vec![
            UsageEntry {
                at: 100,
                model: "claude-opus-4-8".into(),
                usage: real_turn(),
            },
            UsageEntry {
                at: 200,
                model: "some-future-model".into(),
                usage: real_turn(),
            },
        ];

        assert!(near(total(&table, &entries).unwrap(), 0.293_654));
    }
}
