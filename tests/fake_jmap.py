"""A small JMAP mail server for the end-to-end scripts: enough of RFC 8620/8621 for the kernel's `jmap` bridge. Two accounts
(ann / pw-ann, dan / pw-dan), each with an inbox, drafts, sent and trash. Not a mail server: nothing leaves this process."""
import base64, json, threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

USERS = {"ann": "pw-ann", "dan": "pw-dan"}


class Store:
    def __init__(self):
        self.lock = threading.Lock()
        self.counter = 0
        self.boxes = {u: {r: f"{u}-{r}" for r in ("inbox", "drafts", "sent", "trash")} for u in USERS}
        self.emails = {u: {} for u in USERS}
        self.submitted = []  # what was sent: {user, to, cc, subject, text, headers}
        self.requests = 0
        self.blobs = {}  # blobId -> (type, bytes)

    def add(self, user, subject, text, sender="zed@client.test", name="Zed", seen=False, mailbox="inbox", at="2026-10-18T10:00:00Z", html=None, files=()):
        self.counter += 1
        eid = f"e{self.counter}"
        atts = []
        for fname, ftype, data in files:
            blob = f"b{len(self.blobs) + 1}"
            self.blobs[blob] = (ftype, data)
            atts.append({"blobId": blob, "name": fname, "type": ftype, "size": len(data)})
        body = [{"partId": "t", "type": "text/html" if html else "text/plain"}]
        self.emails[user][eid] = {
            "id": eid, "mailboxIds": {self.boxes[user][mailbox]: True}, "keywords": {"$seen": True} if seen else {},
            "from": [{"name": name, "email": sender}], "to": [{"name": user, "email": f"{user}@acme.test"}], "cc": [], "subject": subject,
            "receivedAt": at, "preview": (text or "")[:60], "hasAttachment": bool(atts), "attachments": atts, "messageId": [f"{eid}@client.test"], "references": None,
            "textBody": body, "bodyValues": {"t": {"value": html or text}},
        }
        return eid


STORE = Store()


def method(user, name, args):
    s = STORE
    emails, boxes = s.emails[user], s.boxes[user]
    if name == "Mailbox/get":
        out = []
        for role, mid in boxes.items():
            mine = [e for e in emails.values() if mid in e["mailboxIds"]]
            out.append({"id": mid, "name": role.capitalize(), "role": role, "totalEmails": len(mine), "unreadEmails": len([e for e in mine if "$seen" not in e["keywords"]]), "sortOrder": list(boxes).index(role)})
        return {"list": out}
    if name == "Identity/get":
        return {"list": [{"id": f"id-{user}", "name": user.capitalize(), "email": f"{user}@acme.test"}]}
    if name == "Email/query":
        f = args.get("filter", {})
        rows = [e for e in emails.values() if (not f.get("inMailbox") or f["inMailbox"] in e["mailboxIds"]) and (not f.get("text") or f["text"].lower() in (e["subject"] + e["bodyValues"]["t"]["value"]).lower())]
        rows.sort(key=lambda e: e["receivedAt"], reverse=True)
        pos, lim = args.get("position", 0), args.get("limit", 25)
        return {"ids": [e["id"] for e in rows[pos:pos + lim]], "total": len(rows)}
    if name == "Email/get":
        ids = args.get("ids") or []
        return {"list": [emails[i] for i in ids if i in emails], "notFound": [i for i in ids if i not in emails]}
    if name == "Email/set":
        res = {"updated": {}, "notUpdated": {}, "created": {}, "notCreated": {}}
        for eid, patch in (args.get("update") or {}).items():
            if eid not in emails:
                res["notUpdated"][eid] = {"type": "notFound"}
                continue
            apply(emails[eid], patch)
            res["updated"][eid] = None
        for cid, e in (args.get("create") or {}).items():
            s.counter += 1
            eid = f"e{s.counter}"
            e = dict(e, id=eid, receivedAt="2026-10-18T12:00:00Z", preview=e["bodyValues"]["t"]["value"][:60], hasAttachment=bool(e.get("attachments")), messageId=[f"{eid}@acme.test"], references=None)
            emails[eid] = e
            res["created"][cid] = {"id": eid}
            s.draft_ids = getattr(s, "draft_ids", {})
            s.draft_ids[cid] = eid
        return res
    if name == "EmailSubmission/set":
        res = {"created": {}, "notCreated": {}}
        for cid, sub in (args.get("create") or {}).items():
            eid = sub["emailId"]
            if eid.startswith("#"):
                eid = s.draft_ids[eid[1:]]
            e = emails[eid]
            s.submitted.append({"user": user, "to": [t["email"] for t in e["to"]], "cc": [t["email"] for t in e.get("cc") or []], "subject": e["subject"], "text": e["bodyValues"]["t"]["value"],
                                "attachments": [(a["name"], a["type"], s.blobs[a["blobId"]][1]) for a in e.get("attachments") or []], "in_reply_to": e.get("header:In-Reply-To:asMessageIds"), "references": e.get("header:References:asMessageIds"), "from": e["from"][0]["email"]})
            res["created"][cid] = {"id": f"sub{len(s.submitted)}"}
            for ref, patch in (args.get("onSuccessUpdateEmail") or {}).items():
                apply(e, patch)
        return res
    raise KeyError(name)


def apply(email, patch):
    for k, v in patch.items():
        if k == "mailboxIds":
            email["mailboxIds"] = dict(v)
        elif k.startswith("mailboxIds/"):
            box = k.split("/", 1)[1]
            if v is None:
                email["mailboxIds"].pop(box, None)
            else:
                email["mailboxIds"][box] = True
        elif k.startswith("keywords/"):
            kw = k.split("/", 1)[1]
            if v is None:
                email["keywords"].pop(kw, None)
            else:
                email["keywords"][kw] = True


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *a):
        pass

    def user(self):
        h = self.headers.get("Authorization", "")
        if not h.startswith("Basic "):
            return None
        name, _, pw = base64.b64decode(h[6:]).decode().partition(":")
        return name if USERS.get(name) == pw else None

    def send(self, code, body):
        raw = json.dumps(body).encode()
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(raw)))
        self.end_headers()
        self.wfile.write(raw)

    def do_GET(self):
        STORE.requests += 1
        if self.path == "/.well-known/jmap":
            self.send(307 if False else 200, {"apiUrl": f"http://127.0.0.1:{self.server.server_port}/api", "primaryAccounts": {"urn:ietf:params:jmap:mail": "acct"}, "downloadUrl": f"http://127.0.0.1:{self.server.server_port}/dl/{{accountId}}/{{blobId}}/{{name}}?type={{type}}", "uploadUrl": f"http://127.0.0.1:{self.server.server_port}/up/{{accountId}}"}) if self.user() else self.send(401, {})
        elif self.path.startswith("/dl/") and self.user():
            blob = self.path.split("/")[3]
            kind, data = STORE.blobs[blob]
            self.send_response(200)
            self.send_header("Content-Type", kind)
            self.send_header("Content-Length", str(len(data)))
            self.end_headers()
            self.wfile.write(data)
        else:
            self.send(404, {})

    def do_POST(self):
        user = self.user()
        if not user:
            return self.send(401, {})
        if self.path.startswith("/up/"):
            data = self.rfile.read(int(self.headers.get("Content-Length", 0)))
            with STORE.lock:
                blob = f"b{len(STORE.blobs) + 1}"
                STORE.blobs[blob] = (self.headers.get("Content-Type", "application/octet-stream"), data)
            return self.send(200, {"accountId": "acct", "blobId": blob, "size": len(data)})
        body = json.loads(self.rfile.read(int(self.headers.get("Content-Length", 0))))
        out, results = [], {}
        with STORE.lock:
            for name, args, cid in body["methodCalls"]:
                for k in list(args):
                    if k.startswith("#"):
                        ref = args.pop(k)
                        args[k[1:]] = results[ref["resultOf"]]["ids"]
                try:
                    res = method(user, name, args)
                except KeyError:
                    out.append(["error", {"type": "unknownMethod"}, cid])
                    continue
                results[cid] = res
                out.append([name, res, cid])
        self.send(200, {"methodResponses": out})


def start(port=19080):
    server = ThreadingHTTPServer(("127.0.0.1", port), Handler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    return server
