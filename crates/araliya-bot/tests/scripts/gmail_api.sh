#!/usr/bin/env bash
# Smoke-test key Gmail API endpoints using the cached access token.
# Run from the repo root:  bash tests/scripts/gmail_api.sh
#
# Reads:  config/gmail_token.json   (access_token)
#         .env                      (GOOGLE_CLIENT_ID, GOOGLE_CLIENT_SECRET for reference)
#
# Requires: curl, jq

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
TOKEN_FILE="$REPO_ROOT/config/gmail_token.json"
ENV_FILE="$REPO_ROOT/.env"
GMAIL_BASE="https://gmail.googleapis.com/gmail/v1/users/me"

# ── helpers ──────────────────────────────────────────────────────────────────

ok()   { printf '\033[32m  OK\033[0m  %s\n' "$*"; }
fail() { printf '\033[31m FAIL\033[0m %s\n' "$*"; }
hdr()  { printf '\n\033[1m=== %s ===\033[0m\n' "$*"; }

# Load .env if present (silently skip missing vars)
if [[ -f "$ENV_FILE" ]]; then
    # shellcheck disable=SC2046
    export $(grep -v '^#' "$ENV_FILE" | grep -v '^$' | xargs) 2>/dev/null || true
fi

# Read access token from cached token file
if [[ ! -f "$TOKEN_FILE" ]]; then
    echo "token file not found: $TOKEN_FILE"
    echo "run the bot once to complete OAuth, then retry"
    exit 1
fi

ACCESS_TOKEN=$(jq -r '.access_token' "$TOKEN_FILE")
if [[ -z "$ACCESS_TOKEN" || "$ACCESS_TOKEN" == "null" ]]; then
    echo "access_token missing in $TOKEN_FILE"
    exit 1
fi

AUTH_HEADER="Authorization: Bearer $ACCESS_TOKEN"

# ── 1. list labels ────────────────────────────────────────────────────────────

hdr "1. users.labels.list"
LABELS_JSON=$(curl -sf -H "$AUTH_HEADER" "$GMAIL_BASE/labels")
LABEL_COUNT=$(echo "$LABELS_JSON" | jq '.labels | length')
ok "got $LABEL_COUNT labels"

echo "$LABELS_JSON" | jq -r '.labels[] | "\(.id)\t\(.name)"' | sort -t$'\t' -k2 | head -30
echo "(first 30 shown)"

# Look up the ID for a specific label name (pass via TEST_LABEL env var)
TEST_LABEL="${TEST_LABEL:-}"
LABEL_ID=""
if [[ -n "$TEST_LABEL" ]]; then
    LABEL_ID=$(echo "$LABELS_JSON" | jq -r --arg name "$TEST_LABEL" \
        '.labels[] | select(.name == $name) | .id')
    if [[ -n "$LABEL_ID" && "$LABEL_ID" != "null" ]]; then
        ok "resolved label '$TEST_LABEL' -> $LABEL_ID"
    else
        fail "label '$TEST_LABEL' not found"
    fi
fi

# ── 2. messages.list via labelIds ─────────────────────────────────────────────

hdr "2. users.messages.list  (labelIds=INBOX, maxResults=3)"
LIST_JSON=$(curl -sf -H "$AUTH_HEADER" \
    "$GMAIL_BASE/messages?labelIds=INBOX&maxResults=3")
MSG_COUNT=$(echo "$LIST_JSON" | jq '.messages | length')
ok "got $MSG_COUNT message stubs"
echo "$LIST_JSON" | jq '.messages'

if [[ -n "$LABEL_ID" ]]; then
    hdr "2b. messages.list  (labelIds=INBOX+$LABEL_ID)"
    LIST2=$(curl -sf -H "$AUTH_HEADER" \
        "$GMAIL_BASE/messages?labelIds=INBOX&labelIds=$LABEL_ID&maxResults=5")
    COUNT2=$(echo "$LIST2" | jq '.messages | length')
    ok "got $COUNT2 message stubs for label '$TEST_LABEL'"
    echo "$LIST2" | jq '.messages'
fi

# ── helpers ───────────────────────────────────────────────────────────────────

urlencode() {
    python3 -c "import urllib.parse,sys; print(urllib.parse.quote(sys.argv[1]))" "$1" 2>/dev/null \
        || printf '%s' "$1" | sed 's/ /%20/g;s/"/%22/g;s/:/%3A/g'
}

q_list() {
    # q_list <description> <q-value> <maxResults>
    local desc="$1" q="$2" max="${3:-5}"
    local encoded; encoded=$(urlencode "$q")
    local json; json=$(curl -sf -H "$AUTH_HEADER" \
        "$GMAIL_BASE/messages?q=$encoded&maxResults=$max")
    local count; count=$(echo "$json" | jq '.messages | length // 0')
    ok "$desc  →  $count results   (q=$q)"
    echo "$json" | jq '.messages // []'
    echo "$json"   # captured by caller when needed
}

# ── 3. q= method tests (always run) ──────────────────────────────────────────

hdr "3. q= search method tests"

# 3a. basic in:inbox
q_list "in:inbox" "in:inbox" 3

# 3b. explicit is:unread
q_list "in:inbox is:unread" "in:inbox is:unread" 3

# 3c. label by display name (what the bot currently uses)
if [[ -n "$TEST_LABEL" ]]; then
    Q_LABEL_NAME=$(urlencode "label:\"$TEST_LABEL\"")
    Q_JSON_NAME=$(curl -sf -H "$AUTH_HEADER" \
        "$GMAIL_BASE/messages?q=$Q_LABEL_NAME&maxResults=5")
    Q_COUNT_NAME=$(echo "$Q_JSON_NAME" | jq '.messages | length // 0')
    ok "q=label:\"$TEST_LABEL\"  →  $Q_COUNT_NAME results"
    echo "$Q_JSON_NAME" | jq '.messages // []'

    # 3d. compare: labelIds= vs q=label: — do they return the same IDs?
    if [[ -n "$LABEL_ID" ]]; then
        hdr "3d. result comparison: labelIds= vs q=label: for '$TEST_LABEL'"
        LID_JSON=$(curl -sf -H "$AUTH_HEADER" \
            "$GMAIL_BASE/messages?labelIds=INBOX&labelIds=$LABEL_ID&maxResults=10")
        LID_IDS=$(echo "$LID_JSON"  | jq -r '[.messages[]?.id] | sort | .[]')
        Q_IDS=$(echo "$Q_JSON_NAME" | jq -r '[.messages[]?.id] | sort | .[]')

        IN_BOTH=$(comm -12 <(echo "$LID_IDS") <(echo "$Q_IDS") | wc -l | tr -d ' ')
        ONLY_LID=$(comm -23 <(echo "$LID_IDS") <(echo "$Q_IDS") | wc -l | tr -d ' ')
        ONLY_Q=$(comm -13  <(echo "$LID_IDS") <(echo "$Q_IDS") | wc -l | tr -d ' ')

        ok "matching IDs: $IN_BOTH"
        [[ "$ONLY_LID" -eq 0 ]] && ok "only in labelIds: 0" \
            || fail "only in labelIds ($ONLY_LID): $(comm -23 <(echo "$LID_IDS") <(echo "$Q_IDS"))"
        [[ "$ONLY_Q" -eq 0 ]] && ok "only in q=: 0" \
            || fail "only in q= ($ONLY_Q): $(comm -13 <(echo "$LID_IDS") <(echo "$Q_IDS"))"
    fi
fi

# ── 4. message metadata fetch ─────────────────────────────────────────────────

hdr "4. users.messages.get  (format=metadata, first message from step 2)"
FIRST_ID=$(echo "$LIST_JSON" | jq -r '.messages[0].id // empty')
if [[ -n "$FIRST_ID" ]]; then
    MSG_JSON=$(curl -sf -H "$AUTH_HEADER" \
        "$GMAIL_BASE/messages/$FIRST_ID?format=metadata&metadataHeaders=Subject&metadataHeaders=From&metadataHeaders=Date")
    SUBJECT=$(echo "$MSG_JSON" | jq -r \
        '.payload.headers[] | select(.name == "Subject") | .value')
    FROM=$(echo "$MSG_JSON" | jq -r \
        '.payload.headers[] | select(.name == "From") | .value')
    ok "id=$FIRST_ID"
    ok "From: $FROM"
    ok "Subject: $SUBJECT"
    echo "$MSG_JSON" | jq '{id, threadId, internalDate, snippet}'
else
    fail "no messages returned in step 2"
fi

echo ""
echo "done."
