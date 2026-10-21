"""Personal mailboxes, end to end over HTTP: each person connects their own login (checked with the mail server, kept encrypted,
never shown again), reads only their own mail, answers and writes. A small JMAP server (fake_jmap.py) plays the mail server."""
import json, urllib.request, urllib.parse, urllib.error
import fake_jmap
from e2e_lib import *

PLUGIN = "atrium_mailbox"
PORT = 19080


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


server = fake_jmap.start(PORT)
S = fake_jmap.STORE
owner, bob, cy = login(USER), login("bob"), login("cy")
ann_1 = S.add("ann", "Invoice " + RUN, "Where is my invoice?\nThanks", sender="zed@client.test", name="Zed")
ann_2 = S.add("ann", "Newsletter", "", html="<p>Big <b>news</b></p><script>alert(1)</script>", sender="news@shop.test", name="Shop", seen=True, at="2026-10-17T09:00:00Z")
dan_1 = S.add("dan", "Dan only " + RUN, "private to dan")

for who in (bob, cy):
    who.call("DELETE", "/api/ui/bridge-credentials/jmap")
code, out = owner.call("PATCH", "/api/ui/bridges/jmap", {"enabled": True})
check("an administrator switches the mailbox bridge on", code == 200, (code, out))
code, out = owner.call("PUT", "/api/settings/bridge.jmap.session_url", {"value": f"http://127.0.0.1:{PORT}/.well-known/jmap"})
check("and gives the mail server's address", code in (200, 204), (code, out))

# --- not connected yet
check("before connecting, the plugin says so", ok(bob, "mailbox_state")["connected"] is False)
code, out = call(bob, "inbox_messages")
check("and listing mail is refused with a reason", code != 200 and "connected" in str(out), (code, out))

# --- connecting
code, out = bob.call("PUT", "/api/ui/bridge-credentials/jmap", {"username": "ann", "secret": "wrong"})
check("a login the mail server refuses is not kept", code == 422, (code, out))
code, out = bob.call("GET", "/api/ui/bridge-credentials/jmap")
check("nothing was stored", code == 200 and out["connected"] is False, (code, out))
code, out = bob.call("PUT", "/api/ui/bridge-credentials/stripe", {"username": "a", "secret": "b"})
check("only bridges that work for a person take a login", code == 404, (code, out))
code, out = bob.call("PUT", "/api/ui/bridge-credentials/jmap", {"username": "ann", "secret": "pw-ann"})
check("a correct login is kept", code == 200 and out["connected"] is True and out["username"] == "ann", (code, out))
code, out = bob.call("GET", "/api/ui/bridge-credentials/jmap")
check("it shows who, never the password", out["connected"] and out["username"] == "ann" and "pw-ann" not in json.dumps(out), out)
check("the plugin sees the mailbox connected", ok(bob, "mailbox_state") == {"connected": True, "name": "ann"})
code, out = cy.call("GET", "/api/ui/bridge-credentials/jmap")
check("another person has no login", out["connected"] is False, out)
code, out = cy.call("PUT", "/api/ui/bridge-credentials/jmap", {"username": "dan", "secret": "pw-dan"})
check("each person connects their own", code == 200, (code, out))

# --- reading, only your own mail
rows = ok(bob, "inbox_messages")
check("bob reads ann's mail: newest first, unread tagged", [r["id"] for r in rows] == [ann_1, ann_2] and rows[0]["tag"] == "New" and rows[1]["tag"] == "", rows)
check("cy reads dan's mail, not ann's", [r["id"] for r in ok(cy, "inbox_messages")] == [dan_1])
code, out = call(cy, "get_message", {"id": ann_1})
check("cy cannot open a message of ann's mailbox", code != 200, (code, out))
check("a search finds by words", [r["id"] for r in ok(bob, "inbox_messages", {"q": "invoice"})] == [ann_1])
check("folders list with unread counts", any(f["name"].startswith("Inbox (1)") for f in ok(bob, "folders")), ok(bob, "folders"))
m = ok(bob, "get_message", {"id": ann_1})
check("a message shows who, when and its text quoted", m["subject"] == "Invoice " + RUN and "Zed <zed@client.test>" in m["from"] and "> Where is my invoice?" in m["body"], m)
check("opening it marked it read", S.emails["ann"][ann_1]["keywords"].get("$seen") is True)
h = ok(bob, "get_message", {"id": ann_2})
check("mail with only HTML is shown as text, without scripts", "Big news" in h["body"] and "alert" not in h["body"] and "<" not in h["body"], h)
ok(bob, "mark_unread", {"id": ann_1})
check("a message can be marked unread again", "$seen" not in S.emails["ann"][ann_1]["keywords"])

# --- answering and writing
code, out = call(bob, "reply_message", {"id": ann_1, "text": "  "})
check("an empty answer is refused", code != 200, (code, out))
ok(bob, "reply_message", {"id": ann_1, "text": "It was paid on Monday."})
sent = S.submitted[-1]
check("the answer goes to the sender, from the person's own address, as a reply", sent["to"] == ["zed@client.test"] and sent["from"] == "ann@acme.test" and sent["subject"] == "Re: Invoice " + RUN and sent["in_reply_to"] == [f"{ann_1}@client.test"], sent)
check("and sits in the sent folder, not the drafts", any(list(e["mailboxIds"]) == ["ann-sent"] for e in S.emails["ann"].values() if e["subject"].startswith("Re: Invoice")))
code, out = call(bob, "send_message", {"to": "", "subject": "x", "text": "y"})
check("a new message needs an address", code != 200, (code, out))
code, out = call(bob, "send_message", {"to": "a@x.test\r\nBcc: evil@x.test", "subject": "x", "text": "y"})
check("an address with a header injection is refused", code != 200, (code, out))
ok(bob, "send_message", {"to": "kim@x.test, lee@x.test", "cc": "pat@x.test", "subject": "Hello " + RUN, "text": "Hi both"})
sent = S.submitted[-1]
check("a new message goes to everyone named", sent["to"] == ["kim@x.test", "lee@x.test"] and sent["cc"] == ["pat@x.test"] and sent["subject"] == "Hello " + RUN, sent)
check("it was sent from cy's mailbox account? no: from bob's own (ann)", sent["user"] == "ann")
ok(cy, "send_message", {"to": "kim@x.test", "subject": "From dan", "text": "hi"})
check("each person sends as themselves", S.submitted[-1]["user"] == "dan" and S.submitted[-1]["from"] == "dan@acme.test")

# --- trash
ok(bob, "trash_message", {"id": ann_2})
check("a message moves to the trash", list(S.emails["ann"][ann_2]["mailboxIds"]) == ["ann-trash"])
check("and leaves the inbox", [r["id"] for r in ok(bob, "inbox_messages")] == [ann_1])


# --- attachments
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


with_file = S.add("ann", "Contract " + RUN, "See attached", files=[("terms.txt", "text/plain", b"Terms and conditions\n")])
got = ok(bob, "get_message", {"id": with_file})
check("a message lists its attachments", "terms.txt" in got["files"], got)
fetched = ok(bob, "get_attachments", {"id": with_file})
check("opening the attachments stores each one", len(fetched["files"]) == 1 and fetched["files"][0]["name"] == "terms.txt" and fetched["files"][0]["size"] == 21, fetched)
key = fetched["files"][0]["key"]
code, data = fetch(bob, key)
check("the person who asked can download it", code == 200 and data == b"Terms and conditions\n", (code, data))
code, data = fetch(owner, key)
check("another member cannot", code in (403, 404), (code, data))
code, data = fetch(cy, key)
check("nor a person with a mailbox of their own", code in (403, 404), (code, data))
code, out = call(cy, "get_attachments", {"id": with_file})
check("and cy sees only dan's mailbox, so that message is not there", code != 200, (code, out))

code, up = upload(bob, "notes.txt", b"My notes\n", guard=PLUGIN)
check("a person attaches a file on the form (guarded upload)", code in (200, 201), (code, up))
ok(bob, "send_message", {"to": "kim@x.test", "subject": "With file " + RUN, "text": "see attached", "files": [up]})
sent = S.submitted[-1]
check("a new message carries the attachment to the mail server", sent["attachments"] == [("notes.txt", sent["attachments"][0][1], b"My notes\n")], sent)
code, up2 = upload(bob, "more.txt", b"More\n", guard=PLUGIN)
ok(bob, "reply_message", {"id": ann_1, "text": "Here", "files": [up2]})
check("so does a reply", S.submitted[-1]["attachments"][0][2] == b"More\n", S.submitted[-1])
code, up3 = upload(owner, "other.txt", b"not yours\n", guard=PLUGIN)
code, out = call(bob, "send_message", {"to": "kim@x.test", "subject": "Steal", "text": "x", "files": [up3]})
check("a file someone else uploaded cannot be attached", code != 200, (code, out))
code, out = call(bob, "send_message", {"to": "kim@x.test", "subject": "Plain", "text": "x", "files": [{"key": "media/2026/10/nope.txt", "name": "x.txt"}]})
check("nor a key the plugin never guarded", code != 200, (code, out))

# --- the kernel hides the login from plugins and forgets it on request
code, out = bob.call("DELETE", "/api/ui/bridge-credentials/jmap")
check("a person can disconnect", code == 200 and out["connected"] is False, (code, out))
code, out = call(bob, "inbox_messages")
check("after that, the mailbox is closed to the plugin again", code != 200, (code, out))
code, out = cy.call("GET", "/api/ui/bridge-credentials/jmap")
check("others are not affected", out["connected"] is True, out)
server.shutdown()
finish()
