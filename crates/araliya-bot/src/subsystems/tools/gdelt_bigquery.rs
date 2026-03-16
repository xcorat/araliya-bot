//! GDELT v2 BigQuery tool.
//!
//! Fetches recent global-news events from the public GDELT v2 dataset using
//! the BigQuery REST API, authenticated via the service-account key stored at
//! `config/secrets/araliya-1012f47de255.json`.
//!
//! Auth flow:
//!   service-account JSON  →  RS256 JWT  →  OAuth2 token endpoint  →  access_token
//!   access_token  →  BigQuery `runQuery` REST endpoint  →  rows

use chrono::Utc;
use jsonwebtoken::{Algorithm, EncodingKey, Header};
use serde::{Deserialize, Serialize};

// ── Service-account JSON ──────────────────────────────────────────────────────

#[derive(Deserialize)]
struct ServiceAccount {
    client_email: String,
    private_key: String,
    token_uri: String,
    project_id: String,
}

fn load_service_account() -> Result<ServiceAccount, String> {
    let raw = std::fs::read_to_string("config/secrets/araliya-1012f47de255.json")
        .map_err(|e| format!("gdelt: failed to read service account: {e}"))?;
    serde_json::from_str(&raw)
        .map_err(|e| format!("gdelt: failed to parse service account JSON: {e}"))
}

// ── JWT claims ────────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct JwtClaims {
    iss: String,
    scope: String,
    aud: String,
    iat: i64,
    exp: i64,
}

fn build_jwt(sa: &ServiceAccount) -> Result<String, String> {
    let now = Utc::now().timestamp();
    let claims = JwtClaims {
        iss: sa.client_email.clone(),
        scope: "https://www.googleapis.com/auth/bigquery.readonly".to_string(),
        aud: sa.token_uri.clone(),
        iat: now,
        exp: now + 3600,
    };
    let key = EncodingKey::from_rsa_pem(sa.private_key.as_bytes())
        .map_err(|e| format!("gdelt: failed to load RSA key: {e}"))?;
    jsonwebtoken::encode(&Header::new(Algorithm::RS256), &claims, &key)
        .map_err(|e| format!("gdelt: JWT signing failed: {e}"))
}

// ── Token exchange ────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
}

async fn fetch_access_token(sa: &ServiceAccount) -> Result<String, String> {
    let jwt = build_jwt(sa)?;
    let client = build_http_client()?;
    let resp = client
        .post(&sa.token_uri)
        .form(&[
            ("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer"),
            ("assertion", &jwt),
        ])
        .send()
        .await
        .map_err(|e| format!("gdelt: token request failed: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("gdelt: token endpoint {status}: {body}"));
    }

    let tok: TokenResponse = resp
        .json()
        .await
        .map_err(|e| format!("gdelt: failed to parse token response: {e}"))?;
    Ok(tok.access_token)
}

// ── BigQuery query ────────────────────────────────────────────────────────────

/// Public arguments for a GDELT fetch — all fields are optional.
#[derive(Debug, Default, Deserialize)]
pub struct GdeltQueryArgs {
    /// How many minutes back to include (default 60).
    pub lookback_minutes: Option<u32>,
    /// Maximum rows to return (default 50).
    pub limit: Option<u32>,
    /// Only include events with at least this many articles.
    pub min_articles: Option<u32>,
    /// Only include events whose ABS(GoldsteinScale) is at least this value (0-10).
    /// Goldstein scale measures event stability impact; 0 = any, 10 = most extreme.
    pub min_importance: Option<f32>,
    /// When true sort results by ABS(GoldsteinScale) DESC (importance) then NumArticles DESC.
    /// When false (default) sort by NumArticles DESC only.
    pub sort_by_importance: Option<bool>,
    /// When true restrict results to events covered by English-language sources,
    /// by joining with the `gdeltv2.eventmentions` table on MentionLanguage = 'eng'.
    pub english_only: Option<bool>,
}

/// A single GDELT event row.
#[derive(Debug, Serialize, Deserialize)]
pub struct GdeltEvent {
    pub date: String,
    pub actor1: String,
    pub actor2: String,
    pub event_code: String,
    pub goldstein: f64,
    pub num_articles: u64,
    pub avg_tone: f64,
    pub source_url: String,
}

#[derive(Serialize)]
struct BqQueryBody {
    query: String,
    #[serde(rename = "useLegacySql")]
    use_legacy_sql: bool,
    #[serde(rename = "timeoutMs")]
    timeout_ms: u64,
    #[serde(rename = "maxResults")]
    max_results: u32,
}

#[derive(Deserialize)]
struct BqResponse {
    #[serde(default)]
    rows: Vec<BqRow>,
}

#[derive(Deserialize)]
struct BqRow {
    f: Vec<BqCell>,
}

#[derive(Deserialize)]
struct BqCell {
    v: Option<serde_json::Value>,
}

fn cell_str(row: &BqRow, idx: usize) -> String {
    row.f
        .get(idx)
        .and_then(|c| c.v.as_ref())
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

fn cell_f64(row: &BqRow, idx: usize) -> f64 {
    cell_str(row, idx).parse().unwrap_or(0.0)
}

fn cell_u64(row: &BqRow, idx: usize) -> u64 {
    cell_str(row, idx).parse().unwrap_or(0)
}

fn build_query(args: &GdeltQueryArgs) -> String {
    let lookback_minutes = args.lookback_minutes.unwrap_or(60);
    let limit = args.limit.unwrap_or(50);

    // WHERE clause extras
    let min_articles_clause = args
        .min_articles
        .map(|n| format!("  AND NumArticles >= {n}\n"))
        .unwrap_or_default();

    let min_importance_clause = args
        .min_importance
        .map(|v| format!("  AND ABS(GoldsteinScale) >= {v}\n"))
        .unwrap_or_default();

    // English-only: restrict to events that have at least one English-language mention.
    let english_join = if args.english_only.unwrap_or(false) {
        format!(
            "INNER JOIN (
    SELECT DISTINCT GlobalEventID
    FROM `gdelt-bq.gdeltv2.eventmentions`
    WHERE MentionLanguage = 'eng'
      AND DATEADDED >= UNIX_SECONDS(TIMESTAMP_SUB(CURRENT_TIMESTAMP(), INTERVAL {lookback_minutes} MINUTE))
  ) eng_mentions USING (GLOBALEVENTID)\n"
        )
    } else {
        String::new()
    };

    // ORDER BY
    let order_clause = if args.sort_by_importance.unwrap_or(false) {
        "ABS(GoldsteinScale) DESC, NumArticles DESC"
    } else {
        "NumArticles DESC"
    };

    format!(
        r#"SELECT
  CAST(SQLDATE AS STRING) AS date,
  IFNULL(Actor1Name, '') AS actor1,
  IFNULL(Actor2Name, '') AS actor2,
  IFNULL(EventCode, '') AS event_code,
  IFNULL(GoldsteinScale, 0.0) AS goldstein,
  IFNULL(NumArticles, 0) AS num_articles,
  IFNULL(AvgTone, 0.0) AS avg_tone,
  IFNULL(SOURCEURL, '') AS source_url
FROM `gdelt-bq.gdeltv2.events`
{english_join}WHERE DATEADDED >= UNIX_SECONDS(TIMESTAMP_SUB(CURRENT_TIMESTAMP(), INTERVAL {lookback_minutes} MINUTE))
{min_articles_clause}{min_importance_clause}ORDER BY {order_clause}
LIMIT {limit}"#
    )
}

fn build_http_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| format!("gdelt: failed to build HTTP client: {e}"))
}

/// Fetch recent GDELT events using the provided query args.
pub async fn fetch_events(args: &GdeltQueryArgs) -> Result<Vec<GdeltEvent>, String> {
    let sa = load_service_account()?;
    let token = fetch_access_token(&sa).await?;
    let client = build_http_client()?;

    let query = build_query(args);
    let limit = args.limit.unwrap_or(50);
    let url = format!(
        "https://bigquery.googleapis.com/bigquery/v2/projects/{}/queries",
        sa.project_id
    );

    let body = BqQueryBody {
        query,
        use_legacy_sql: false,
        timeout_ms: 30_000,
        max_results: limit,
    };

    let resp = client
        .post(&url)
        .bearer_auth(&token)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("gdelt: BigQuery request failed: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("gdelt: BigQuery error {status}: {text}"));
    }

    let bq: BqResponse = resp
        .json()
        .await
        .map_err(|e| format!("gdelt: failed to parse BigQuery response: {e}"))?;

    let events = bq
        .rows
        .iter()
        .map(|row| GdeltEvent {
            date: cell_str(row, 0),
            actor1: cell_str(row, 1),
            actor2: cell_str(row, 2),
            event_code: cell_str(row, 3),
            goldstein: cell_f64(row, 4),
            num_articles: cell_u64(row, 5),
            avg_tone: cell_f64(row, 6),
            source_url: cell_str(row, 7),
        })
        .collect();

    Ok(events)
}

/// Run a minimal 1-row query to verify BigQuery connectivity.
pub async fn healthcheck() -> Result<String, String> {
    let args = GdeltQueryArgs {
        lookback_minutes: Some(1440), // 24 h
        limit: Some(1),
        min_articles: None,
        min_importance: None,
        sort_by_importance: None,
        english_only: None,
    };
    let events = fetch_events(&args).await?;
    if events.is_empty() {
        Ok("gdelt: reachable, no rows returned".to_string())
    } else {
        Ok(format!("gdelt: reachable, sample date={}", events[0].date))
    }
}
