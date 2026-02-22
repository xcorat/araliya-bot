use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Deserialize;
use serde::Serialize;
use tracing::debug;

use crate::config::NewsmailAggregatorConfig;

use super::gmail::{self, GmailFilter, GmailSummary};

/// Accepts either a single label ID string or an array of label ID strings.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum LabelArg {
    One(String),
    Many(Vec<String>),
}

impl LabelArg {
    fn into_vec(self) -> Vec<String> {
        match self {
            LabelArg::One(s) => vec![s],
            LabelArg::Many(v) => v,
        }
    }
}

#[derive(Debug, Deserialize, Default)]
struct NewsmailArgs {
    /// One label ID string or an array of label ID strings.
    label: Option<LabelArg>,
    n_last: Option<usize>,
    t_interval: Option<String>,
    tsec_last: Option<u64>,
    q: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ResolvedNewsmailConfig {
    /// Label IDs to filter by.
    labels: Vec<String>,
    n_last: usize,
    tsec_last: Option<u64>,
    q: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct HealthcheckResult {
    pub ok: bool,
    pub filter: String,
    pub sample: Option<GmailSummary>,
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::from_secs(0))
        .as_secs()
}

fn parse_interval_to_secs(input: &str) -> Option<u64> {
    let raw = input.trim().to_ascii_lowercase();
    if raw.is_empty() {
        return None;
    }

    if let Ok(v) = raw.parse::<u64>() {
        return (v > 0).then_some(v);
    }

    let split_at = raw.chars().take_while(|ch| ch.is_ascii_digit()).count();
    if split_at == 0 || split_at >= raw.len() {
        return None;
    }

    let amount = raw[..split_at].parse::<u64>().ok()?;
    if amount == 0 {
        return None;
    }

    let unit = raw[split_at..].trim();
    let multiplier = match unit {
        "s" | "sec" | "secs" | "second" | "seconds" => 1,
        "m" | "min" | "mins" | "minute" | "minutes" => 60,
        "h" | "hr" | "hrs" | "hour" | "hours" => 60 * 60,
        "d" | "day" | "days" => 60 * 60 * 24,
        "w" | "week" | "weeks" => 60 * 60 * 24 * 7,
        "mon" | "month" | "months" => 60 * 60 * 24 * 30,
        _ => return None,
    };

    amount.checked_mul(multiplier)
}

fn resolve_config(defaults: NewsmailAggregatorConfig, args_json: &str) -> ResolvedNewsmailConfig {
    let args = serde_json::from_str::<NewsmailArgs>(args_json).unwrap_or_default();

    let labels = args
        .label
        .map(|v| {
            v.into_vec()
                .into_iter()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or_else(|| defaults.label_ids.clone());

    let n_last = args
        .n_last
        .unwrap_or(defaults.n_last)
        .clamp(1, 100);

    let tsec_last = args
        .t_interval
        .as_deref()
        .and_then(parse_interval_to_secs)
        .or(args.tsec_last)
        .or(defaults.tsec_last)
        .filter(|v| *v > 0);

    let q = args
        .q
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .or_else(|| defaults.q.filter(|v| !v.is_empty()));

    ResolvedNewsmailConfig {
        labels,
        n_last,
        tsec_last,
        q,
    }
}

pub async fn get(defaults: NewsmailAggregatorConfig, args_json: &str) -> Result<Vec<GmailSummary>, String> {
    debug!(args_json = %args_json, "newsmail: raw args JSON");
    let resolved = resolve_config(defaults, args_json);
    debug!(
        labels = ?resolved.labels,
        n_last = resolved.n_last,
        tsec_last = ?resolved.tsec_last,
        q = ?resolved.q,
        "newsmail: resolved config"
    );

    debug!(label_ids = ?resolved.labels, q = ?resolved.q, "newsmail: fetching");

    let filter = GmailFilter {
        label_ids: resolved.labels.clone(),
        q: resolved.q.clone(),
    };
    let mut items = gmail::read_many(filter, resolved.n_last as u32).await?;

    if let Some(window_secs) = resolved.tsec_last {
        let cutoff = now_unix().saturating_sub(window_secs);
        items.retain(|item| item.internal_date_unix.map(|ts| ts >= cutoff).unwrap_or(false));
    }

    Ok(items)
}

pub async fn healthcheck(defaults: NewsmailAggregatorConfig) -> Result<HealthcheckResult, String> {
    let filter = GmailFilter {
        label_ids: defaults.label_ids.clone(),
        q: Some("newsletter".to_string()),
    };
    let filter_desc = format!("labelIds={:?} q=newsletter", defaults.label_ids);
    let mut items = gmail::read_many(filter, 1).await?;
    let sample = items.drain(..).next();

    Ok(HealthcheckResult {
        ok: sample.is_some(),
        filter: filter_desc,
        sample,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn defaults() -> NewsmailAggregatorConfig {
        NewsmailAggregatorConfig {
            label_ids: vec!["INBOX".to_string()],
            n_last: 10,
            tsec_last: Some(600),
            q: None,
        }
    }

    #[test]
    fn resolve_uses_tsec_last_key() {
        let resolved = resolve_config(defaults(), r#"{"tsec_last": 120, "n_last": 5}"#);
        assert_eq!(resolved.tsec_last, Some(120));
        assert_eq!(resolved.n_last, 5);
    }

    #[test]
    fn resolve_uses_t_interval_key() {
        let resolved = resolve_config(defaults(), r#"{"t_interval": "1d"}"#);
        assert_eq!(resolved.tsec_last, Some(86_400));
    }

    #[test]
    fn resolve_ignores_zero_tsec_last() {
        let resolved = resolve_config(defaults(), r#"{"tsec_last": 0}"#);
        assert_eq!(resolved.tsec_last, None);
    }

    #[test]
    fn resolve_falls_back_to_default_label_ids_when_no_label_arg() {
        let resolved = resolve_config(defaults(), "{}");
        assert_eq!(resolved.labels, vec!["INBOX".to_string()]);
    }

    #[test]
    fn resolve_uses_label_array() {
        let resolved = resolve_config(
            defaults(),
            r#"{"label": ["Label_111", "Label_222"]}"#,
        );
        assert_eq!(resolved.labels, vec!["Label_111".to_string(), "Label_222".to_string()]);
    }

    #[test]
    fn resolve_falls_back_to_defaults_on_invalid_json() {
        let resolved = resolve_config(defaults(), "not-json");
        assert_eq!(resolved.labels, vec!["INBOX".to_string()]);
        assert_eq!(resolved.n_last, 10);
        assert_eq!(resolved.tsec_last, Some(600));
    }

    #[test]
    fn resolve_uses_label_key() {
        let resolved = resolve_config(defaults(), r#"{"label": "n/News"}"#);
        assert_eq!(resolved.labels, vec!["n/News".to_string()]);
    }

    #[test]
    fn resolve_uses_q_from_args() {
        let resolved = resolve_config(defaults(), r#"{"q": "is:unread"}"#);
        assert_eq!(resolved.q.as_deref(), Some("is:unread"));
    }

    #[test]
    fn resolve_uses_q_from_defaults() {
        let mut d = defaults();
        d.q = Some("is:unread".to_string());
        let resolved = resolve_config(d, "{}");
        assert_eq!(resolved.q.as_deref(), Some("is:unread"));
    }

    #[test]
    fn parse_interval_examples() {
        assert_eq!(parse_interval_to_secs("1min"), Some(60));
        assert_eq!(parse_interval_to_secs("1d"), Some(86_400));
        assert_eq!(parse_interval_to_secs("1mon"), Some(2_592_000));
    }
}
