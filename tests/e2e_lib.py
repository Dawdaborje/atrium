"""Helpers for the end-to-end scripts in this folder: a browser-like client with cookies and CSRF, a
script-like client with only a bearer token, and a tiny checker. See README.md.

Plain Python 3, no packages. AETHER_URL, AETHER_USER and AETHER_PASSWORD override the defaults."""
import http.cookiejar, json, os, sys, urllib.request, urllib.error

BASE = os.environ.get("AETHER_URL", "http://127.0.0.1:17892")
USER = os.environ.get("AETHER_USER", "owner")
PASSWORD = os.environ.get("AETHER_PASSWORD", "Test-pass-123")
RUN = os.urandom(3).hex()
failures = []

def check(name, ok, detail=""):
    print(("ok   " if ok else "FAIL ") + name + ("" if ok else f"  -> {detail}"))
    if not ok:
        failures.append(name)

class Browser:
    def __init__(self):
        self.jar = http.cookiejar.CookieJar()
        self.opener = urllib.request.build_opener(urllib.request.HTTPCookieProcessor(self.jar))
    def csrf(self):
        for c in self.jar:
            if c.name == "aether_csrf":
                return c.value
    def call(self, method, path, body=None, csrf=True, headers=None):
        h = {"Content-Type": "application/json", "Origin": BASE}
        if csrf and self.csrf():
            h["X-CSRF-Token"] = self.csrf()
        h.update(headers or {})
        req = urllib.request.Request(BASE + path, data=json.dumps(body).encode() if body is not None else None, method=method, headers=h)
        try:
            with self.opener.open(req) as r:
                return r.status, json.loads(r.read() or b"null")
        except urllib.error.HTTPError as e:
            raw = e.read()
            try:
                return e.code, json.loads(raw)
            except Exception:
                return e.code, raw.decode(errors="replace")

def finish():
    print("\n%d failure(s)" % len(failures))
    sys.exit(1 if failures else 0)

def machine(method, path, token, body=None, extra=None):
    """A script: no cookies, no Origin, no CSRF token; only the bearer."""
    h = {"Content-Type": "application/json"}
    if token:
        h["Authorization"] = "Bearer " + token
    h.update(extra or {})
    req = urllib.request.Request(BASE + path, data=json.dumps(body).encode() if body is not None else None, method=method, headers=h)
    try:
        with urllib.request.urlopen(req) as r:
            return r.status, json.loads(r.read() or b"null")
    except urllib.error.HTTPError as e:
        raw = e.read()
        try:
            return e.code, json.loads(raw)
        except Exception:
            return e.code, raw.decode(errors="replace")

