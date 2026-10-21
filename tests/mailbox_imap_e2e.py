"""Personal mailboxes on an IMAP server, end to end over HTTP: the `imap` bridge reads over IMAP and sends over SMTP with the person's own
login. fake_imap.py plays both servers. The same plugin (`atrium_mailbox`) as for JMAP; it moves on to `imap` when no JMAP login is connected."""
import json, urllib.request, urllib.parse, urllib.error
import fake_imap
from e2e_lib import *

PLUGIN = "atrium_mailbox"
IMAP_PORT, SMTP_PORT = 19143, 19025


def login(user):
    b = Browser()
    b.call("GET", "/api/auth/methods")
    code, body = b.call("POST", "/api/auth/login", {"username": user, "password": PASSWORD})
    assert code == 200, (user, code, body)
    return b


def call(b, fn, body=None):
    return b.call("POST", f"/api/plugins/{PLUGIN}/{fn}", body or {})


def ok(b, fn, body=None):
    code, out = call(b, fn, body)
    assert code == 200, (fn, code, out)
    return out


def upload(b, name, data, guard=None):
    url = BASE + "/api/media?name=" + urllib.parse.quote(name) + (f"&guard={guard}" if guard else "")
    req = urllib.request.Request(url, data=data, method="POST", headers={"Content-Type": "text/plain", "Origin": BASE, "X-CSRF-Token": b.csrf()})
    try:
        with b.opener.open(req) as r:
            return r.status, json.loads(r.read())
    except urllib.error.HTTPError as e:
        return e.code, e.read()


def fetch(b, key):
    req = urllib.request.Request(BASE + "/api/media/" + key, headers={"Origin": BASE})
    try:
        with b.opener.open(req) as r:
            return r.status, r.read()
    except urllib.error.HTTPError as e:
        return e.code, e.read()


servers = fake_imap.start(IMAP_PORT, SMTP_PORT)
S = fake_imap.STORE
owner, bob, cy = login(USER), login("bob"), login("cy")
for who in (bob, cy):
    who.call("DELETE", "/api/ui/bridge-credentials/jmap")
    who.call("DELETE", "/api/ui/bridge-credentials/imap")
ann_1 = S.add("ann", "Invoice " + RUN, "Where is my invoice?\nThanks", sender="Zed <zed@client.test>")
ann_2 = S.add("ann", "Newsletter", "", html="<p>Big <b>news</b></p><script>alert(1)</script>", sender="news@shop.test", seen=True, date="17-Oct-2026 09:00:00 +0000")
ann_3 = S.add("ann", "Contract " + RUN, "See attached", files=[("terms.txt", "text/plain", b"Terms and conditions\n")], date="18-Oct-2026 09:00:00 +0000")
ann_4 = S.add("ann", "Café menu", "", raw=b"From: =?UTF-8?Q?Ren=C3=A9?= <rene@client.test>\r\nSubject: =?UTF-8?Q?Caf=C3=A9_menu?=\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Transfer-Encoding: quoted-printable\r\n\r\nCr=C3=A8me br=C3=BBl=C3=A9e\r\n", date="16-Oct-2026 09:00:00 +0000")
dan_1 = S.add("dan", "Dan only " + RUN, "private to dan")

code, out = owner.call("PATCH", "/api/ui/bridges/imap", {"enabled": True})
check("an administrator switches the IMAP bridge on", code == 200, (code, out))
for k, v in {"host": "127.0.0.1", "port": str(IMAP_PORT), "security": "none", "smtp_host": "127.0.0.1", "smtp_port": str(SMTP_PORT), "smtp_security": "none", "domain": "acme.test"}.items():
    code, out = owner.call("PUT", f"/api/settings/bridge.imap.{k}", {"value": v})
    assert code in (200, 204), (k, code, out)

# --- connecting
check("before connecting, the plugin says so", ok(bob, "mailbox_state")["connected"] is False)
code, out = bob.call("PUT", "/api/ui/bridge-credentials/imap", {"username": "ann", "secret": "wrong"})
check("a login the server refuses is not kept", code == 422, (code, out))
code, out = bob.call("PUT", "/api/ui/bridge-credentials/imap", {"username": "ann", "secret": "pw-ann"})
check("a correct login is kept", code == 200 and out["connected"] is True, (code, out))
code, out = bob.call("GET", "/api/ui/bridge-credentials/imap")
check("it shows who, never the password", out["connected"] and "pw-ann" not in json.dumps(out), out)
check("the plugin sees the mailbox connected (through the imap bridge)", ok(bob, "mailbox_state") == {"connected": True, "name": "ann"})
code, out = cy.call("PUT", "/api/ui/bridge-credentials/imap", {"username": "dan", "secret": "pw-dan"})
check("cy connects to dan's mailbox", code == 200, (code, out))

# --- reading
folders = ok(bob, "folders")
check("folders list with unread counts", [f["name"] for f in folders] == ["INBOX (3)", "Drafts (0)", "Sent (0)", "Trash (0)"], folders)
inbox = ok(bob, "inbox_messages")
check("the inbox lists the newest first", [r["subject"] for r in inbox] == ["Café menu", "Contract " + RUN, "Newsletter", "Invoice " + RUN], inbox)
check("all four are there, none of dan's", len(inbox) == 4 and all("Dan only" not in r["subject"] for r in inbox), inbox)
by = {r["subject"]: r for r in inbox}
check("a coded subject and sender are decoded", "Café menu" in by and by["Café menu"]["from"] == "René <rene@client.test>", by)
check("unread mail is tagged", by["Invoice " + RUN]["tag"] == "New" and by["Newsletter"]["tag"] == "", by)
check("a message with a file says so", by["Contract " + RUN]["files"] == "Yes" and by["Invoice " + RUN]["files"] == "", by)
check("a search finds by words", [r["subject"] for r in ok(bob, "inbox_messages", {"q": "invoice"})] == ["Invoice " + RUN])
check("and by words that are not ASCII", [r["subject"] for r in ok(bob, "inbox_messages", {"q": "Café"})] == ["Café menu"])
check("cy sees dan's mail, not ann's", [r["subject"] for r in ok(cy, "inbox_messages")] == ["Dan only " + RUN])
code, out = call(cy, "get_message", {"id": by["Invoice " + RUN]["id"].split(".")[0] + ".494e424f58"})
check("an id of ann's mail does not open it for dan", code != 200 or "Invoice" not in json.dumps(out), (code, out))

m = ok(bob, "get_message", {"id": by["Invoice " + RUN]["id"]})
check("a message shows who, when and its text quoted", m["from"] == "Zed <zed@client.test>" and m["body"] == "> Where is my invoice?\n> Thanks" and m["when"] == "2026-10-19 10:00", m)
check("opening it marked it read", S.boxes["ann"]["INBOX"][0]["flags"] >= {"\\Seen"})
news = ok(bob, "get_message", {"id": by["Newsletter"]["id"]})
check("mail with only HTML is shown as text, without scripts", "Big news" in news["body"] and "alert" not in news["body"] and "<" not in news["body"], news)
fr = ok(bob, "get_message", {"id": by["Café menu"]["id"]})
check("quoted-printable text is decoded", "Crème brûlée" in fr["body"], fr)
ok(bob, "mark_unread", {"id": by["Invoice " + RUN]["id"]})
check("a message can be marked unread again", "\\Seen" not in S.boxes["ann"]["INBOX"][0]["flags"])

# --- attachments
got = ok(bob, "get_message", {"id": by["Contract " + RUN]["id"]})
check("a message lists its attachments", "terms.txt" in got["files"], got)
fetched = ok(bob, "get_attachments", {"id": by["Contract " + RUN]["id"]})
check("opening the attachments stores each one", len(fetched["files"]) == 1 and fetched["files"][0]["name"] == "terms.txt", fetched)
key = fetched["files"][0]["key"]
code, data = fetch(bob, key)
check("the person who asked can download it", code == 200 and data == b"Terms and conditions\n", (code, data))
code, data = fetch(owner, key)
check("another member cannot", code in (403, 404), (code, data))

# --- answering and writing (SMTP, with the person's own login)
code, out = call(bob, "reply_message", {"id": by["Invoice " + RUN]["id"], "text": ""})
check("an empty answer is refused", code != 200, (code, out))
ok(bob, "reply_message", {"id": by["Invoice " + RUN]["id"], "text": "It was paid on Monday."})
sent = S.submitted[-1]
msg = sent["message"]
check("the answer goes to the sender, from the person's own address", sent["rcpts"] == ["zed@client.test"] and sent["from"] == "ann@acme.test" and sent["user"] == "ann", sent["rcpts"])
check("as a reply: Re: subject, In-Reply-To set", msg["Subject"] == "Re: Invoice " + RUN and "invoice" not in (msg["In-Reply-To"] or "").lower() and "client.test" in (msg["In-Reply-To"] or ""), (msg["Subject"], msg["In-Reply-To"]))
check("and a copy is kept in the Sent folder", any(b"It was paid on Monday." in x["raw"] for x in S.boxes["ann"]["Sent"]))
code, out = call(bob, "send_message", {"to": "", "subject": "x", "text": "y"})
check("a new message needs an address", code != 200, (code, out))
code, out = call(bob, "send_message", {"to": "a@x.test\r\nBcc: evil@x.test", "subject": "x", "text": "y"})
check("an address with a header injection is refused", code != 200, (code, out))
ok(bob, "send_message", {"to": "kim@x.test, lee@x.test", "cc": "pat@x.test", "subject": "Hello " + RUN, "text": "Hi both"})
sent = S.submitted[-1]
check("a new message goes to everyone named", sorted(sent["rcpts"]) == ["kim@x.test", "lee@x.test", "pat@x.test"], sent["rcpts"])
ok(cy, "send_message", {"to": "kim@x.test", "subject": "From dan", "text": "hi"})
check("each person sends as themselves", S.submitted[-1]["user"] == "dan" and S.submitted[-1]["from"] == "dan@acme.test")
code, up = upload(bob, "notes.txt", b"My notes\n", guard=PLUGIN)
ok(bob, "send_message", {"to": "kim@x.test", "subject": "With file " + RUN, "text": "see attached", "files": [up]})
atts = [p for p in S.submitted[-1]["message"].iter_attachments()]
check("an attached file reaches the mail server", len(atts) == 1 and atts[0].get_filename() == "notes.txt" and atts[0].get_content().strip() == "My notes", [a.get_filename() for a in atts])

# --- trash
ok(bob, "trash_message", {"id": by["Newsletter"]["id"]})
check("a message moves to the trash", any(x["uid"] == ann_2 for x in S.boxes["ann"]["Trash"]))
check("and leaves the inbox", all(x["uid"] != ann_2 for x in S.boxes["ann"]["INBOX"]))

# --- disconnecting
code, out = bob.call("DELETE", "/api/ui/bridge-credentials/imap")
check("a person can disconnect", code == 200 and out["connected"] is False, (code, out))
code, out = call(bob, "inbox_messages")
check("after that, the mailbox is closed to the plugin again", code != 200, (code, out))
code, out = cy.call("GET", "/api/ui/bridge-credentials/imap")
check("others are not affected", out["connected"] is True, out)
for s in servers:
    s.shutdown()
finish()
