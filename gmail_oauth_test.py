#!/usr/bin/env python3
"""Minimal Gmail OAuth (Desktop + loopback callback) that reads one message.

Required env:
  GOOGLE_CLIENT_ID

Optional env:
  GOOGLE_CLIENT_SECRET
"""

from __future__ import annotations

import argparse
import base64
import hashlib
import http.server
import json
import os
import secrets
import socket
import threading
import time
import urllib.parse
import webbrowser
from pathlib import Path

import requests

AUTH_URL = "https://accounts.google.com/o/oauth2/v2/auth"
TOKEN_URL = "https://oauth2.googleapis.com/token"
GMAIL_BASE = "https://gmail.googleapis.com/gmail/v1/users/me"
SCOPE = "https://www.googleapis.com/auth/gmail.readonly"
CALLBACK_PATH = "/oauth2/callback"
TOKEN_FILE = Path(__file__).with_name("gmail_token.json")


class OAuthResult:
    def __init__(self, expected_state: str):
        self.expected_state = expected_state
        self.code: str | None = None
        self.error: str | None = None


def _free_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.bind(("127.0.0.1", 0))
        return int(s.getsockname()[1])


def _make_pkce() -> tuple[str, str]:
    verifier = secrets.token_urlsafe(64)
    digest = hashlib.sha256(verifier.encode("ascii")).digest()
    challenge = base64.urlsafe_b64encode(digest).rstrip(b"=").decode("ascii")
    return verifier, challenge


def _build_auth_url(client_id: str, redirect_uri: str, state: str, code_challenge: str) -> str:
    params = {
        "client_id": client_id,
        "redirect_uri": redirect_uri,
        "response_type": "code",
        "scope": SCOPE,
        "state": state,
        "access_type": "offline",
        "prompt": "consent",
        "code_challenge": code_challenge,
        "code_challenge_method": "S256",
    }
    return f"{AUTH_URL}?{urllib.parse.urlencode(params)}"


def _oauth_server_once(host: str, port: int, result: OAuthResult) -> None:
    class Handler(http.server.BaseHTTPRequestHandler):
        def do_GET(self) -> None:  # noqa: N802
            parsed = urllib.parse.urlparse(self.path)
            if parsed.path != CALLBACK_PATH:
                self.send_response(404)
                self.end_headers()
                return

            params = urllib.parse.parse_qs(parsed.query)
            state = params.get("state", [None])[0]
            code = params.get("code", [None])[0]
            error = params.get("error", [None])[0]

            if error:
                result.error = error
            elif state != result.expected_state:
                result.error = "state_mismatch"
            elif code:
                result.code = code
            else:
                result.error = "missing_code"

            self.send_response(200)
            self.send_header("Content-type", "text/html")
            self.end_headers()
            self.wfile.write(
                b"<html><body><h1>OAuth Complete</h1><p>You can close this tab and return to the terminal.</p></body></html>"
            )

        def log_message(self, fmt: str, *args: object) -> None:
            return

    server = http.server.HTTPServer((host, port), Handler)
    server.handle_request()
    server.server_close()


def _exchange_code(client_id: str, client_secret: str | None, code: str, code_verifier: str, redirect_uri: str) -> dict:
    data = {
        "client_id": client_id,
        "code": code,
        "code_verifier": code_verifier,
        "redirect_uri": redirect_uri,
        "grant_type": "authorization_code",
    }
    if client_secret:
        data["client_secret"] = client_secret
    response = requests.post(TOKEN_URL, data=data, timeout=30)
    response.raise_for_status()
    return response.json()


def _refresh_access_token(client_id: str, client_secret: str | None, refresh_token: str) -> dict:
    data = {
        "client_id": client_id,
        "refresh_token": refresh_token,
        "grant_type": "refresh_token",
    }
    if client_secret:
        data["client_secret"] = client_secret
    response = requests.post(TOKEN_URL, data=data, timeout=30)
    response.raise_for_status()
    return response.json()


def _load_token() -> dict | None:
    if not TOKEN_FILE.exists():
        return None
    return json.loads(TOKEN_FILE.read_text(encoding="utf-8"))


def _save_token(token: dict) -> None:
    TOKEN_FILE.write_text(json.dumps(token, indent=2), encoding="utf-8")


def _ensure_access_token(client_id: str, client_secret: str | None, force_auth: bool) -> str:
    token = None if force_auth else _load_token()

    if token and "refresh_token" in token:
        expires_at = token.get("expires_at", 0)
        now = int(time.time())
        if token.get("access_token") and expires_at > now + 60:
            return token["access_token"]

        refreshed = _refresh_access_token(client_id, client_secret, token["refresh_token"])
        token["access_token"] = refreshed["access_token"]
        token["expires_at"] = int(time.time()) + int(refreshed.get("expires_in", 3600))
        _save_token(token)
        return token["access_token"]

    host = "127.0.0.1"
    port = _free_port()
    redirect_uri = f"http://{host}:{port}{CALLBACK_PATH}"
    state = secrets.token_urlsafe(24)
    code_verifier, code_challenge = _make_pkce()
    auth_url = _build_auth_url(client_id, redirect_uri, state, code_challenge)
    result = OAuthResult(expected_state=state)

    print("\n=== Gmail OAuth (Desktop App) ===")
    print("Add this exact callback in Google Cloud OAuth client settings:")
    print(f"  {redirect_uri}")

    thread = threading.Thread(target=_oauth_server_once, args=(host, port, result), daemon=True)
    thread.start()

    print("\nOpening browser for consent...")
    print(auth_url)
    webbrowser.open(auth_url)

    thread.join(timeout=300)
    if result.error:
        raise RuntimeError(f"oauth failed: {result.error}")
    if not result.code:
        raise RuntimeError("oauth failed: no code received (timeout or callback mismatch)")

    token = _exchange_code(client_id, client_secret, result.code, code_verifier, redirect_uri)
    token["expires_at"] = int(time.time()) + int(token.get("expires_in", 3600))
    _save_token(token)
    return token["access_token"]


def _header(payload: dict, name: str) -> str:
    headers = payload.get("headers", [])
    for entry in headers:
        if entry.get("name", "").lower() == name.lower():
            return entry.get("value", "")
    return ""


def read_one_email(access_token: str, query: str = "in:inbox") -> None:
    headers = {"Authorization": f"Bearer {access_token}"}

    list_resp = requests.get(
        f"{GMAIL_BASE}/messages",
        headers=headers,
        params={"maxResults": 1, "q": query},
        timeout=30,
    )
    list_resp.raise_for_status()
    messages = list_resp.json().get("messages", [])
    if not messages:
        print("No messages found for query:", query)
        return

    msg_id = messages[0]["id"]
    get_resp = requests.get(
        f"{GMAIL_BASE}/messages/{msg_id}",
        headers=headers,
        params={"format": "metadata", "metadataHeaders": ["Subject", "From", "Date"]},
        timeout=30,
    )
    get_resp.raise_for_status()
    msg = get_resp.json()
    payload = msg.get("payload", {})

    print("\n=== Read One Message ===")
    print("id:", msg.get("id"))
    print("threadId:", msg.get("threadId"))
    print("from:", _header(payload, "From"))
    print("subject:", _header(payload, "Subject"))
    print("date:", _header(payload, "Date"))
    print("snippet:", msg.get("snippet", ""))


def _self_test() -> None:
    verifier, challenge = _make_pkce()
    assert 43 <= len(verifier) <= 128
    assert len(challenge) >= 43
    print("self-test ok")


def main() -> int:
    parser = argparse.ArgumentParser(description="Minimal Gmail OAuth read-one tester")
    parser.add_argument("--force-auth", action="store_true", help="Ignore cached token and run OAuth again")
    parser.add_argument("--query", default="in:inbox", help="Gmail search query for selecting one message")
    parser.add_argument("--self-test", action="store_true", help="Run local non-network self tests and exit")
    args = parser.parse_args()

    if args.self_test:
        _self_test()
        return 0

    client_id = os.getenv("GOOGLE_CLIENT_ID", "").strip()
    client_secret = os.getenv("GOOGLE_CLIENT_SECRET", "").strip() or None
    if not client_id:
        print("Missing GOOGLE_CLIENT_ID env var")
        return 2

    try:
        access_token = _ensure_access_token(client_id, client_secret, args.force_auth)
        read_one_email(access_token, query=args.query)
        print("\nDone.")
        return 0
    except requests.HTTPError as e:
        body = ""
        if e.response is not None:
            body = e.response.text
        print(f"HTTP error: {e}\n{body}")
        return 1
    except Exception as e:
        print(f"Error: {e}")
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
