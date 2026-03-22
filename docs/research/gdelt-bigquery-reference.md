# GDELT BigQuery Reference

**Sources:**
- https://blog.gdeltproject.org/a-compilation-of-gdelt-bigquery-demos/
- https://blog.gdeltproject.org/the-datasets-of-gdelt-as-of-february-2016/
- https://blog.gdeltproject.org/google-bigquery-gkg-2-0-sample-queries/
- https://blog.gdeltproject.org/gdelt-2-0-our-global-world-in-realtime/

---

## Overview

GDELT (Global Database of Events, Language, and Tone) is a realtime open data index over global human society, cataloging events, emotions, images, and narratives from mass live data mining of the world's public information streams. It spans news, television, images, books, and academic literature. All datasets are publicly available via Google BigQuery.

---

## Datasets

### GDELT 2.0 Event Database
- **Type:** Event data
- **Time range:** February 2015 – present (historical backfile to 1979 planned)
- **Update interval:** Every 15 minutes
- **Source:** Worldwide news in 100 languages; 65 are live machine-translated at 100% volume via GDELT Translingual
- **Scale (as of Feb 2016):** 326 million mentions of 103 million distinct events
- **BigQuery table:** `gdelt-bq:gdeltv2.events`
- **Codebook:** http://data.gdeltproject.org/documentation/GDELT-Event_Codebook-V2.0.pdf
- **Key addition over v1:** Separate `MENTIONS` table recording every re-mention of each event with timestamp, offset, context, and confidence. Enables tracking how events propagate through media over time.

### GDELT 2.0 Global Knowledge Graph (GKG)
- **Type:** Knowledge graph — extracts persons, organizations, locations, themes (2,300+), emotions from each article
- **Time range:** February 2015 – present
- **Update interval:** Every 15 minutes
- **Scale:** 200+ million records; growing at 500K–1M articles/day
- **BigQuery table:** `gdelt-bq:gdeltv2.gkg`
- **Codebook:** http://data.gdeltproject.org/documentation/GDELT-Global_Knowledge_Graph_Codebook-V2.1.pdf
- **GCAM Codebook (emotions):** http://data.gdeltproject.org/documentation/GCAM-MASTER-CODEBOOK.TXT

### GDELT 2.0 Event Mentions
- **BigQuery table:** `gdelt-bq:gdeltv2.eventmentions`
- Tracks every mention of every event, not just first occurrence
- Updated every 15 minutes

### GDELT 1.0 Event Database
- **Type:** Event data
- **Time range:** January 1979 – present
- **Update interval:** Daily
- **Source:** Worldwide news in 100 languages; hand-translated foreign language content only (for full machine-translation volume, use v2.0)
- **Scale:** 3.5 billion mentions of 364 million distinct events
- **BigQuery:** https://cloudplatform.googleblog.com/2014/05/worlds-largest-event-dataset-now-publicly-available-in-google-bigquery.html
- **Format codebook:** http://data.gdeltproject.org/documentation/GDELT-Data_Format_Codebook.pdf
- **CSV column headers (1979–Mar 2013 back file):** http://gdeltproject.org/data/lookups/CSV.header.historical.txt
- **CSV column headers (Apr 2013–present):** http://gdeltproject.org/data/lookups/CSV.header.dailyupdates.txt
- **Event taxonomy:** CAMEO (300+ event types, hierarchical numeric codes)
  - CAMEO manual: http://data.gdeltproject.org/documentation/CAMEO.Manual.1.1b3.pdf
  - Event code lookup: http://gdeltproject.org/data/lookups/CAMEO.eventcodes.txt
  - Goldstein scale lookup: http://gdeltproject.org/data/lookups/CAMEO.goldsteinscale.txt
  - Country codes: http://gdeltproject.org/data/lookups/CAMEO.country.txt
  - Type codes: http://gdeltproject.org/data/lookups/CAMEO.type.txt
  - Known group codes: http://gdeltproject.org/data/lookups/CAMEO.knowngroup.txt
  - Ethnic codes: http://gdeltproject.org/data/lookups/CAMEO.ethnic.txt
  - Religion codes: http://gdeltproject.org/data/lookups/CAMEO.religion.txt

### Visual Global Knowledge Graph (VGKG)
- **Type:** Image annotations via Google Cloud Vision API
- **Time range:** December 2015 – present
- **Update interval:** Every 15 minutes
- **Annotations include:** Objects/activities, OCR, content-based geolocation, logo detection, facial sentiment, landmark detection, SafeSearch
- **Docs:** http://blog.gdeltproject.org/gdelt-visual-knowledge-graph-vgkg-v1-0-available/

### American Television GKG
- **Type:** GKG over American TV news (Internet Archive Television News Archive)
- **Time range:** July 2009 – present
- **Update interval:** Daily with 48-hour embargo
- **Source:** 100+ US television stations; English only; from closed captioning streams
- **Docs:** http://blog.gdeltproject.org/announcing-the-american-television-global-knowledge-graph-tv-gkg/

### Africa and Middle East Academic Literature GKG
- **Time range:** 1950–2012 (some back to 1906)
- **Scale:** 21+ billion words from JSTOR, DTIC, CORE, CiteSeerX, CIA, Internet Archive
- **Special field:** Full extracted citation lists per article
- **Docs:** http://blog.gdeltproject.org/announcing-the-gdelt-2-0-release-of-the-africa-and-middle-east-global-knowledge-graph-ame-gkg/

### Human Rights Knowledge Graph
- **Time range:** 1960–2014
- **Source:** 110,000+ documents from Amnesty International, FIDH, HRW, ICC, ICG, US State Dept, UN
- **Docs:** http://blog.gdeltproject.org/announcing-the-new-human-rights-global-knowledge-graph-hr-gkg/

### Historical American Books Archive
- **Time range:** 1800–2015
- **Source:** 3.5 million Internet Archive + HathiTrust English public domain volumes
- **Docs:** http://blog.gdeltproject.org/3-5-million-books-1800-2015-gdelt-processes-internet-archive-and-hathitrust-book-archives-and-available-in-google-bigquery/

---

## BigQuery Table Reference

| Table | BQ Path | Update |
|-------|---------|--------|
| GDELT 2.0 Events | `gdelt-bq:gdeltv2.events` | 15 min |
| GDELT 2.0 Mentions | `gdelt-bq:gdeltv2.eventmentions` | 15 min |
| GDELT 2.0 GKG | `gdelt-bq:gdeltv2.gkg` | 15 min |

Direct BigQuery links:
- Events: https://bigquery.cloud.google.com/table/gdelt-bq:gdeltv2.events
- Mentions: https://bigquery.cloud.google.com/table/gdelt-bq:gdeltv2.eventmentions
- GKG: https://bigquery.cloud.google.com/table/gdelt-bq:gdeltv2.gkg

---

## Live Data Feeds (CSV, 15-minute updates)

| Feed | URL |
|------|-----|
| Master file list – English | http://data.gdeltproject.org/gdeltv2/masterfilelist.txt |
| Master file list – Translingual | http://data.gdeltproject.org/gdeltv2/masterfilelist-translation.txt |
| Last 15 min – English | http://data.gdeltproject.org/gdeltv2/lastupdate.txt |
| Last 15 min – Translingual | http://data.gdeltproject.org/gdeltv2/lastupdate-translation.txt |

---

## GKG 2.0 Key Fields

### Multi-value delimited fields (semicolon-separated; each entry has `,charoffset` suffix)

| Field | Content |
|-------|---------|
| `V2Themes` | Recognized themes and taxonomy tags |
| `V2Persons` | Person names |
| `V2Organizations` | Organization names |
| `AllNames` | All names (persons + organizations combined) |
| `V2Locations` | Location mentions (pound-sign `#` delimited sub-fields) |
| `TranslationInfo` | Source language code, e.g. `srclc:heb` for Hebrew |
| `GKGRECORDID` | Unique GKG record identifier (used for joins) |
| `SourceCommonName` | Human-readable outlet name |
| `DATE` | Timestamp in `YYYYMMDDHHMMSS` format |

### V2Locations sub-field structure (pound-sign `#` delimited)

```
LocationType # FullName # CountryCode(FIPS10-4) # ADM1Code # ADM2Code(GAUL) # Latitude # Longitude # FeatureID # CharOffset
```

Location types:
- `1` = COUNTRY
- `2` = USSTATE
- `3` = USCITY / landmark
- `4` = WORLDCITY / landmark
- `5` = WORLDSTATE (ADM1 outside US)

Example value: `4#Berlin, Berlin, Germany#GM#GM16#16538#52.5167#13.4#-1746443#1340`

---

## Sample BigQuery Queries

> **Note:** These use the legacy BigQuery SQL syntax from the GDELT blog (2015–2016).
> For current Standard SQL use `gdelt-bq.gdeltv2.gkg` (dot notation, no backets).

### Theme histogram for a person (mention count)
```sql
SELECT theme, COUNT(*) as count
FROM (
  SELECT REGEXP_REPLACE(SPLIT(V2Themes, ';'), r',.*', '') theme
  FROM [gdelt-bq:gdeltv2.gkg]
  WHERE DATE > 20150302000000
    AND DATE < 20150304000000
    AND V2Persons LIKE '%Netanyahu%'
)
GROUP BY theme
ORDER BY 2 DESC
LIMIT 300
```

### Theme histogram (document count — deduplicated per article)
```sql
SELECT theme, COUNT(*) as count
FROM (
  SELECT UNIQUE(REGEXP_REPLACE(SPLIT(V2Themes, ';'), r',.*', '')) theme
  FROM [gdelt-bq:gdeltv2.gkg]
  WHERE DATE > 20150302000000
    AND DATE < 20150304000000
    AND V2Persons LIKE '%Netanyahu%'
)
GROUP BY theme
ORDER BY 2 DESC
LIMIT 300
```

### Person co-occurrence histogram
```sql
SELECT person, COUNT(*) as count
FROM (
  SELECT UNIQUE(REGEXP_REPLACE(SPLIT(V2Persons, ';'), r',.*', '')) person
  FROM [gdelt-bq:gdeltv2.gkg]
  WHERE DATE > 20150302000000
    AND DATE < 20150304000000
    AND V2Persons LIKE '%Netanyahu%'
)
GROUP BY person
ORDER BY 2 DESC
LIMIT 300
```

### Language-filtered theme histogram (Hebrew coverage only)
```sql
SELECT theme, COUNT(*) as count
FROM (
  SELECT UNIQUE(REGEXP_REPLACE(SPLIT(V2Themes, ';'), r',.*', '')) theme
  FROM [gdelt-bq:gdeltv2.gkg]
  WHERE DATE > 20150302000000
    AND DATE < 20150304000000
    AND AllNames LIKE '%Netanyahu%'
    AND TranslationInfo LIKE '%srclc:heb%'
)
GROUP BY theme
ORDER BY 2 DESC
LIMIT 300
```

### Geographic histogram (city-level, any country)
```sql
SELECT location, COUNT(*)
FROM (
  SELECT REGEXP_EXTRACT(SPLIT(V2Locations, ';'), r'^[2-5]#(.*?)#') AS location
  FROM [gdelt-bq:gdeltv2.gkg]
  WHERE DATE > 20150302000000
    AND DATE < 20150304000000
    AND V2Persons LIKE '%Tsipras%'
)
WHERE location IS NOT NULL
GROUP BY location
ORDER BY 2 DESC
LIMIT 100
```

### Geographic histogram filtered by country code (city-level in Greece)
```sql
SELECT location, COUNT(*)
FROM (
  SELECT REGEXP_EXTRACT(SPLIT(V2Locations, ';'), r'^[2-5]#(.*?)#GR#') AS location
  FROM [gdelt-bq:gdeltv2.gkg]
  WHERE DATE > 20150302000000
    AND DATE < 20150304000000
    AND V2Persons LIKE '%Tsipras%'
)
WHERE location IS NOT NULL
GROUP BY location
ORDER BY 2 DESC
LIMIT 100
```

### Lat/long export for mapping
```sql
SELECT coord, COUNT(*)
FROM (
  SELECT REGEXP_REPLACE(
    REGEXP_EXTRACT(SPLIT(V2Locations, ';'), r'^[2-5]#.*?#.*?#.*?#.*?#(.*?#.*?)#'),
    '^(.*?)#(.*?)', '\1;\2'
  ) AS coord
  FROM [gdelt-bq:gdeltv2.gkg]
  WHERE DATE > 20150302000000
    AND DATE < 20150304000000
    AND V2Persons LIKE '%Tsipras%'
)
WHERE coord IS NOT NULL
GROUP BY coord
ORDER BY 2 DESC
LIMIT 100
```

### Person co-occurrence network edge list (for Gephi)
```sql
SELECT a.name, b.name, COUNT(*) as count
FROM (
  FLATTEN(
    SELECT GKGRECORDID,
           UNIQUE(REGEXP_REPLACE(SPLIT(V2Persons, ';'), r',.*', '')) name
    FROM [gdelt-bq:gdeltv2.gkg]
    WHERE DATE > 20150302000000
      AND DATE < 20150304000000
      AND V2Persons LIKE '%Tsipras%',
    name
  )
) a
JOIN EACH (
  SELECT GKGRECORDID,
         UNIQUE(REGEXP_REPLACE(SPLIT(V2Persons, ';'), r',.*', '')) name
  FROM [gdelt-bq:gdeltv2.gkg]
  WHERE DATE > 20150302000000
    AND DATE < 20150304000000
    AND V2Persons LIKE '%Tsipras%'
) b
ON a.GKGRECORDID = b.GKGRECORDID
WHERE a.name < b.name
GROUP EACH BY 1, 2
ORDER BY 3 DESC
LIMIT 250
```

### Three-way join: Events + EventMentions + GKG
```
See: http://blog.gdeltproject.org/complex-queries-combining-events-eventmentions-and-gkg/
```

---

## BigQuery String Function Patterns

| Goal | Pattern |
|------|---------|
| Split semicolon-delimited field | `SPLIT(field, ';')` |
| Strip `,charoffset` suffix from each element | `REGEXP_REPLACE(SPLIT(field,';'), r',.*', '')` |
| Deduplicate per article | Wrap in `UNIQUE(...)` |
| Extract location FullName | `REGEXP_EXTRACT(SPLIT(V2Locations,';'), r'^.*?#(.*?)#')` |
| Extract city-level only (type 2–5) | `REGEXP_EXTRACT(SPLIT(V2Locations,';'), r'^[2-5]#(.*?)#')` |
| Filter by country code | `r'^[2-5]#(.*?)#GR#'` (replace `GR` with FIPS code) |
| Extract lat/lon for mapping | `REGEXP_REPLACE(REGEXP_EXTRACT(...), '^(.*?)#(.*?)', '\1;\2')` |

---

## Normalization Files (GDELT 1.0)

Daily/monthly/yearly event counts for normalizing against monitoring volume growth:

| File | URL |
|------|-----|
| Daily | http://data.gdeltproject.org/normfiles/daily.csv |
| Daily by country | http://data.gdeltproject.org/normfiles/daily_country.csv |
| Monthly | http://data.gdeltproject.org/normfiles/monthly.csv |
| Monthly by country | http://data.gdeltproject.org/normfiles/monthly_country.csv |
| Yearly | http://data.gdeltproject.org/normfiles/yearly.csv |
| Yearly by country | http://data.gdeltproject.org/normfiles/yearly_country.csv |

---

## Analysis Categories & Demo Links

### Getting started
- GKG 2.0 sample queries: http://blog.gdeltproject.org/google-bigquery-gkg-2-0-sample-queries/
- GKG 2.0 + 3.5M books sample queries (includes GCAM emotional timeline): http://blog.gdeltproject.org/google-bigquery-3-5m-books-sample-queries/
- GDELT + Google Cloud Datalab simple timelines: http://blog.gdeltproject.org/getting-started-with-gdelt-google-cloud-datalab-simple-timelines/

### Mapping
- BigQuery UDF + CartoDB one-minute maps: http://blog.gdeltproject.org/new-one-minute-maps-bigquery-udf-cartodb/
- Terascale / petascale cartography overview: http://blog.gdeltproject.org/mapping-at-infinite-scale-terascale-and-petascale-cartography-and-big-data-in-the-bigquery-era/
- Animated map (212 years of books): http://blog.gdeltproject.org/mapping-212-years-of-history-through-books/

### Network visualization
- One-click Gephi network (exports edge CSV): http://blog.gdeltproject.org/one-click-network-visualization-with-bigquerygephi/
- Global influencer network: http://blog.gdeltproject.org/visualizing-the-global-influencer-network/
- Geographic refugee-flow network: http://blog.gdeltproject.org/mapping-the-geographic-networks-of-global-refugee-flows/
- Datalab + GraphViz networks: http://blog.gdeltproject.org/getting-started-with-gdelt-google-cloud-datalab-simple-network-visualizations/

### Sentiment / tone analysis
- Terascale sentiment (341M words/sec): http://blog.gdeltproject.org/terascale-sentiment-analysis-bigquery-tone-coding-books/

### N-gramming
- NGrams at BigQuery scale: http://blog.gdeltproject.org/making-ngrams-bigquery-scale/
- 9.5 billion words of Arabic news: http://blog.gdeltproject.org/ngramming-9-5-billion-words-of-arabic-news/

### Cycles of history
- 2.5M correlations in 2.5 minutes (GDELT 1.0 event cycles): http://blog.gdeltproject.org/towards-psychohistory-uncovering-the-patterns-of-world-history-with-google-bigquery/

### Complex / advanced
- Three-table join (Events + EventMentions + GKG): http://blog.gdeltproject.org/complex-queries-combining-events-eventmentions-and-gkg/
- Analyzing Wayback Machine log files: http://blog.gdeltproject.org/using-bigquery-to-explore-large-log-files-exploring-the-wayback-machine/

---

## GDELT 2.0 Key Capabilities Summary

| Capability | Detail |
|-----------|--------|
| Update cadence | Every 15 minutes (Events, Mentions, GKG) |
| Languages monitored | 100 |
| Languages machine-translated in realtime | 65 (98.4% of non-English volume) |
| Emotion/theme dimensions (GCAM) | 2,300+ from 24 packages |
| Native multilingual sentiment | 15 languages |
| Event categories (CAMEO) | 300+ |
| GKG record count (Feb 2016 baseline) | 200M+; growing 500K–1M/day |
| Geographic resolution | Country → ADM1 → City/Landmark (GNS/GNIS FeatureIDs) |
| Imagery pipeline | Google Cloud Vision API (OCR, labels, faces, landmarks, SafeSearch) |

---

## Relevant Araliya-Bot Integration

The `gdelt_news` agent (`crates/araliya-agents/src/gdelt_news.rs`) is the current integration point. Key config in `config/profiles/newsroom.toml`:

```toml
[agents.newsroom.gdelt_query]
lookback_minutes = 600
limit            = 50
min_articles     = 1
english_only     = true
```

The agent calls `gdelt_bigquery/fetch` via the bus and stores results in an `AgentStore` (SQLite-backed). Raw fetch results are persisted and a summary is LLM-generated from `config/agents/gdelt_news/summary.md`. The `newsroom` agent is a composition on top of `gdelt_news` that adds session/transcript storage and streaming response to the UI.
