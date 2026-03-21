#!/usr/bin/env python3
"""
inspect_agg_pipeline.py — Parse a newsroom/news_aggregator log file and
report per-URL outcomes, cycle summaries, and KG state changes.

Usage:
  python inspect_agg_pipeline.py [LOGFILE] [--out OUTFILE]

Defaults:
  LOGFILE  = logs/test_newsroom.log  (relative to CWD)
  --out    = print to stdout only

NOTE: The stripped article HTML and raw LLM response bodies are NOT present
in the log at debug/info/warn level — only per-URL error messages are logged.
To capture the actual content you would need to add debug-level logging in
do_aggregate() (news_aggregator.rs) before/after the LLM call.

Exit codes:
  0  ok
  1  file not found or no aggregation cycles found
"""

import re
import sys
import argparse
from pathlib import Path
from collections import defaultdict
from dataclasses import dataclass, field
from typing import Optional

# ---------------------------------------------------------------------------
# ANSI strip
# ---------------------------------------------------------------------------
ANSI_ESCAPE = re.compile(r'\x1B(?:[@-Z\\-_]|\[[0-?]*[ -/]*[@-~])')

def strip_ansi(text: str) -> str:
    return ANSI_ESCAPE.sub('', text)

# ---------------------------------------------------------------------------
# Log line pattern
# ---------------------------------------------------------------------------
# 2026-03-17T19:46:57.123456Z LEVEL module::path: MESSAGE
LOG_LINE_RE = re.compile(
    r'^(?P<ts>\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d+Z)\s+'
    r'(?P<level>\w+)\s+'
    r'(?P<module>[^\s:]+):\s+'
    r'(?P<msg>.*)$'
)

# Field extractors (key=value pairs with bare values OR quoted values)
KV_RE = re.compile(r'(\w+)=(?:"([^"]*)"|((?:[^\s"}\]]+)))')

def extract_kv(text: str) -> dict:
    out = {}
    for m in KV_RE.finditer(text):
        key = m.group(1)
        val = m.group(2) if m.group(2) is not None else m.group(3)
        out[key] = val
    return out

# ---------------------------------------------------------------------------
# Data structures
# ---------------------------------------------------------------------------
@dataclass
class UrlOutcome:
    url: str
    outcome: str       # "llm_error" | "non_2xx" | "fetch_error" | "empty_body" | "ok"
    detail: str = ""

@dataclass
class AggCycle:
    ts_start: str
    total_new: int = 0
    outcomes: list = field(default_factory=list)
    processed: int = 0
    skipped: int = 0
    old_cursor: Optional[str] = None
    new_cursor: Optional[str] = None
    kg_rebuilt: bool = False
    kg_error: Optional[str] = None
    no_new_articles: bool = False
    cycle_complete: bool = False
    ts_end: Optional[str] = None

# ---------------------------------------------------------------------------
# Parser
# ---------------------------------------------------------------------------
def parse_log(path: Path) -> list[AggCycle]:
    cycles: list[AggCycle] = []
    current: Optional[AggCycle] = None

    with open(path, 'rb') as f:
        for raw in f:
            try:
                line = strip_ansi(raw.decode('utf-8', errors='replace')).rstrip()
            except Exception:
                continue

            m = LOG_LINE_RE.match(line)
            if not m:
                continue

            ts      = m.group('ts')
            level   = m.group('level')
            module  = m.group('module')
            msg     = m.group('msg')

            # ---------------------------------------------------------------
            # Newsroom trigger
            if 'newsroom' in module and 'triggering news_aggregator' in msg:
                pass  # informational only

            # ---------------------------------------------------------------
            # New aggregation cycle
            if 'news_aggregator' in module and 'starting aggregation' in msg:
                kv = extract_kv(msg)
                cycle = AggCycle(
                    ts_start=ts,
                    total_new=int(kv.get('total_new', 0)),
                )
                cycles.append(cycle)
                current = cycle
                continue

            if current is None:
                continue

            # ---------------------------------------------------------------
            # "No new articles" early return
            if 'news_aggregator' in module and 'no new articles' in msg.lower():
                current.no_new_articles = True

            # ---------------------------------------------------------------
            # Per-URL outcomes
            elif 'news_aggregator' in module and 'LLM summarize' in msg:
                kv = extract_kv(msg)
                url  = kv.get('url', '?')
                # error= may contain spaces; grab everything after error=
                err_match = re.search(r'error=(.+)$', msg)
                err = err_match.group(1).strip() if err_match else kv.get('error', '')
                current.outcomes.append(UrlOutcome(url=url, outcome='llm_error', detail=err))

            elif 'news_aggregator' in module and 'non-2xx' in msg:
                kv = extract_kv(msg)
                url    = kv.get('url', '?')
                status = kv.get('status', '?')
                current.outcomes.append(UrlOutcome(url=url, outcome='non_2xx', detail=f"HTTP {status}"))

            elif 'news_aggregator' in module and re.search(r'\bfetch\b.*\berror\b', msg):
                kv = extract_kv(msg)
                url = kv.get('url', '?')
                err_match = re.search(r'error=(.+)$', msg)
                err = err_match.group(1).strip() if err_match else ''
                current.outcomes.append(UrlOutcome(url=url, outcome='fetch_error', detail=err))

            elif 'news_aggregator' in module and 'stripped article body is empty' in msg:
                kv = extract_kv(msg)
                url = kv.get('url', '?')
                current.outcomes.append(UrlOutcome(url=url, outcome='empty_body', detail=''))

            elif 'news_aggregator' in module and 'article stored' in msg.lower():
                kv = extract_kv(msg)
                url = kv.get('url', '?')
                current.outcomes.append(UrlOutcome(url=url, outcome='ok', detail=''))

            # ---------------------------------------------------------------
            # Cursor advance
            elif 'news_aggregator' in module and 'cursor advanced' in msg:
                kv = extract_kv(msg)
                current.old_cursor = kv.get('old_cursor')
                current.new_cursor = kv.get('new_cursor')

            # ---------------------------------------------------------------
            # KG rebuild
            elif 'news_aggregator' in module and 'KG rebuilt' in msg:
                current.kg_rebuilt = True
            elif 'news_aggregator' in module and 'rebuild_kg' in msg and 'error' in msg.lower():
                err_match = re.search(r'error=(.+)$', msg)
                current.kg_error = err_match.group(1).strip() if err_match else msg

            # ---------------------------------------------------------------
            # Cycle complete
            elif 'news_aggregator' in module and 'aggregation cycle complete' in msg:
                kv = extract_kv(msg)
                current.processed = int(kv.get('processed', 0))
                current.skipped   = int(kv.get('skipped', 0))
                current.cycle_complete = True
                current.ts_end = ts

    return cycles

# ---------------------------------------------------------------------------
# Formatter
# ---------------------------------------------------------------------------
OUTCOME_LABELS = {
    'ok':          '  OK  ',
    'llm_error':   'LLM-ERR',
    'non_2xx':     'HTTP-ERR',
    'fetch_error': 'FETCH-ERR',
    'empty_body':  'EMPTY',
}

def truncate(s: str, n: int = 80) -> str:
    return s if len(s) <= n else s[:n-3] + '...'

def format_report(cycles: list[AggCycle]) -> str:
    lines = []
    lines.append("=" * 80)
    lines.append(f"  NEWSROOM AGGREGATION LOG REPORT  ({len(cycles)} cycle(s) found)")
    lines.append("=" * 80)

    if not cycles:
        lines.append("\nNo aggregation cycles found in log.")
        lines.append(
            "\nTip: The log must contain lines matching:\n"
            "  'news_aggregator: starting aggregation for new articles total_new=N'\n"
            "Check that the log level was at least INFO when the bot ran."
        )
        return '\n'.join(lines)

    for i, cycle in enumerate(cycles, 1):
        lines.append(f"\n{'─'*80}")
        lines.append(f"Cycle #{i}  started={cycle.ts_start}")
        if cycle.ts_end:
            lines.append(f"          ended  ={cycle.ts_end}")
        lines.append(f"  total_new={cycle.total_new}  processed={cycle.processed}  skipped={cycle.skipped}")

        if cycle.no_new_articles:
            lines.append("  [early exit] No new articles in this batch.")

        if cycle.old_cursor is not None or cycle.new_cursor is not None:
            lines.append(f"  cursor: {cycle.old_cursor} → {cycle.new_cursor}")

        if cycle.kg_rebuilt:
            lines.append("  KG: rebuilt successfully")
        elif cycle.kg_error:
            lines.append(f"  KG: ERROR — {truncate(cycle.kg_error)}")
        else:
            lines.append("  KG: not rebuilt (no articles processed or rebuild not logged)")

        if cycle.outcomes:
            lines.append(f"\n  Per-URL outcomes ({len(cycle.outcomes)} entries):")
            lines.append(f"  {'Outcome':<10}  URL")
            lines.append(f"  {'─'*10}  {'─'*60}")
            ok_count    = 0
            error_counts: dict[str, int] = defaultdict(int)
            for o in cycle.outcomes:
                label = OUTCOME_LABELS.get(o.outcome, o.outcome.upper())
                url_short = truncate(o.url, 70)
                lines.append(f"  {label:<10}  {url_short}")
                if o.detail:
                    lines.append(f"  {'':10}  detail: {truncate(o.detail, 70)}")
                if o.outcome == 'ok':
                    ok_count += 1
                else:
                    error_counts[o.outcome] += 1
            lines.append("")
            lines.append(f"  Summary: {ok_count} ok, " + ", ".join(f"{v} {k}" for k,v in error_counts.items()))
        else:
            lines.append("  (no per-URL outcome lines found)")

    lines.append(f"\n{'='*80}")
    lines.append(
        "\nNOTE: Stripped HTML content and raw LLM responses are NOT logged at\n"
        "info/warn/debug level. Only per-URL error messages appear above.\n"
        "To capture actual text/LLM output, add debug! logging in\n"
        "news_aggregator.rs::do_aggregate() around the LLM call."
    )
    return '\n'.join(lines)

# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------
def main():
    ap = argparse.ArgumentParser(description='Parse newsroom aggregation log')
    ap.add_argument('logfile', nargs='?', default='logs/test_newsroom.log',
                    help='Path to log file (default: logs/test_newsroom.log)')
    ap.add_argument('--out', metavar='FILE', help='Write report to FILE (also prints to stdout)')
    args = ap.parse_args()

    path = Path(args.logfile)
    if not path.exists():
        print(f"ERROR: log file not found: {path}", file=sys.stderr)
        sys.exit(1)

    print(f"Parsing {path} ({path.stat().st_size / 1024 / 1024:.1f} MB) ...", file=sys.stderr)
    cycles = parse_log(path)
    report = format_report(cycles)

    print(report)
    if args.out:
        Path(args.out).write_text(report)
        print(f"\nReport written to {args.out}", file=sys.stderr)

    sys.exit(0 if cycles else 1)

if __name__ == '__main__':
    main()
