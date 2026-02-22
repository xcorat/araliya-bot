use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Deserialize;
use serde::Serialize;
use tracing::debug;

use crate::config::NewsmailAggregatorConfig;

use super::gmail::{self, GmailSummary};

#[derive(Debug, Deserialize, Default)]
struct NewsmailArgs {
    label: Option<String>,
    mailbox: Option<String>,
    n_last: Option<usize>,
    t_interval: Option<String>,
    tsec_last: Option<u64>,
    q: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ResolvedNewsmailConfig {
    label: Option<String>,
    mailbox: String,
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

    let label = args
        .label
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());

    let mailbox = args
        .mailbox
        .unwrap_or(defaults.mailbox)
        .trim()
        .to_string();

    let mailbox = if mailbox.is_empty() {
        "inbox".to_string()
    } else {
        mailbox
    };

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
        label,
        mailbox,
        n_last,
        tsec_last,
        q,
    }
}

fn build_query(mailbox: &str, label: Option<&str>, q: Option<&str>) -> String {
    let mut parts = match label {
        Some(lbl) => format!("in:{mailbox} label:\"{lbl}\""),
        None => format!("in:{mailbox}"),
    };
    if let Some(term) = q.filter(|s| !s.is_empty()) {
        parts.push(' ');
        parts.push_str(term);
    }
    parts
}

pub async fn get(defaults: NewsmailAggregatorConfig, args_json: &str) -> Result<Vec<GmailSummary>, String> {
    debug!(args_json = %args_json, "newsmail: raw args JSON");
    let resolved = resolve_config(defaults, args_json);
    debug!(
        mailbox = %resolved.mailbox,
        label = ?resolved.label,
        n_last = resolved.n_last,
        tsec_last = ?resolved.tsec_last,
        q = ?resolved.q,
        "newsmail: resolved config"
    );
    let query = build_query(&resolved.mailbox, resolved.label.as_deref(), resolved.q.as_deref());
    debug!(query = %query, "newsmail: built Gmail query");

    let mut items = gmail::read_many(Some(&query), resolved.n_last as u32).await?;

    if let Some(window_secs) = resolved.tsec_last {
        let cutoff = now_unix().saturating_sub(window_secs);
        items.retain(|item| item.internal_date_unix.map(|ts| ts >= cutoff).unwrap_or(false));
    }

    Ok(items)
}

fn healthcheck_query(mailbox: &str) -> String {
    format!("in:{mailbox} newsletter")
}

pub async fn healthcheck(defaults: NewsmailAggregatorConfig) -> Result<HealthcheckResult, String> {
    let query = healthcheck_query(&defaults.mailbox);
    let mut items = gmail::read_many(Some(&query), 1).await?;
    let sample = items.drain(..).next();

    Ok(HealthcheckResult {
        ok: sample.is_some(),
        filter: query,
        sample,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn defaults() -> NewsmailAggregatorConfig {
        NewsmailAggregatorConfig {
            mailbox: "inbox".to_string(),
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
    fn resolve_falls_back_to_defaults_on_invalid_json() {
        let resolved = resolve_config(defaults(), "not-json");
        assert_eq!(resolved.label, None);
        assert_eq!(resolved.mailbox, "inbox");
        assert_eq!(resolved.n_last, 10);
        assert_eq!(resolved.tsec_last, Some(600));
    }

    #[test]
    fn resolve_uses_label_key() {
        let resolved = resolve_config(defaults(), r#"{"label": "n/News"}"#);
        assert_eq!(resolved.label.as_deref(), Some("n/News"));
    }

    #[test]
    fn build_query_with_label() {
        assert_eq!(build_query("inbox", Some("n/News"), None), "in:inbox label:\"n/News\"");
    }

    #[test]
    fn build_query_with_extra_q() {
        assert_eq!(build_query("inbox", Some("n/News"), Some("is:unread")), "in:inbox label:\"n/News\" is:unread");
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

    #[test]
    fn healthcheck_query_uses_newsletter_filter() {
        assert_eq!(healthcheck_query("inbox"), "in:inbox newsletter");
    }
}
