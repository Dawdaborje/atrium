"""Atrium shared inbox, end to end over HTTP: the administrator switches on mail intake, makes boxes, the provider posts
mail, conversations appear for the people of the box (and only them), answers come back into the same conversation, a
reply is sent, taking/closing/reopening. See README.md."""
import base64, json, time, urllib.error, urllib.request
from e2e_lib import *

PLUGIN = "atrium_mail"
ORG = "atr_org"
DOMAIN = "mail.acme.test"


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


def post_mail(secret, body, tries=3):
    for _ in range(tries):
        code, out = post_mail_once(secret, body)
        if code != 504:  # a cold server is still loading its plugins; a real provider retries too
            break
    return code, out


def post_mail_once(secret, body):
    req = urllib.request.Request(f"{BASE}/api/mail/{ORG}/generic", data=json.dumps(body).encode(), method="POST",
                                 headers={"Content-Type": "application/json", "X-Aether-Mail-Token": secret})
    try:
        with urllib.request.urlopen(req) as r:
            return r.status, json.loads(r.read() or b"null")
    except urllib.error.HTTPError as e:
        return e.code, e.read().decode(errors="replace")


counter = [0]


def mail(to, subject, text="hello", sender="ann@client.test", name="Ann", **extra):
    counter[0] += 1
    m = {"from": {"name": name, "address": sender}, "to": [to], "subject": subject, "message_id": f"<m{counter[0]}.{RUN}@client.test>",
         "text": text, "received_at": "2026-10-18T10:00:00Z"}
    m.update(extra)
    return m


def wait(fn, seconds=30):
    end = time.time() + seconds
    while time.time() < end:
        v = fn()
        if v:
            return v
        time.sleep(0.5)
    return None


owner, bob, cy = login(USER), login("bob"), login("cy")
people = {p["name"]: p["id"] for p in owner.call("POST", "/api/plugins/atrium_chat/list_people", {})[1]}
bob_id, cy_id = people["bob"], people["cy"]

# --- the intake
code, made = owner.call("POST", "/api/ui/mail/secret", {})
secret = made.get("secret")
check("the administrator makes the intake's secret", code in (200, 201) and secret, (code, made))
code, _ = owner.call("PUT", "/api/settings/mail.inbound.domain", {"value": DOMAIN})
check("and sets the mail domain", code in (200, 204), code)
code, _ = owner.call("PUT", "/api/settings/mail.max_per_sender_hour", {"value": 1000})
owner.call("PATCH", "/api/ui/bridges/smtp", {"enabled": True})  # needs a developer account; see setup.sh
for k, v in [("communications.email.provider", "smtp"), ("communications.email.from_address", "inbox@" + DOMAIN), ("bridge.smtp.host", "127.0.0.1"), ("bridge.smtp.port", 1), ("bridge.smtp.security", "none")]:
    owner.call("PUT", f"/api/settings/{k}", {"value": v})

# --- boxes
code, out = call(bob, "create_box", {"name": "Nope"})
check("an ordinary member cannot make a box", code != 200, (code, out))
main = ok(owner, "create_box", {"name": "Support " + RUN, "tag": "", "users": [bob_id]})["id"] if not any(b["tag"] == "" for b in ok(owner, "my_boxes")) else \
    next(b["id"] for b in ok(owner, "my_boxes") if b["tag"] == "")
sales_tag = "sales" + RUN
sales = ok(owner, "create_box", {"name": "Sales " + RUN, "tag": sales_tag, "users": [cy_id]})["id"]
code, out = call(owner, "create_box", {"name": "Again", "tag": sales_tag})
check("a tag is used once", code != 200, (code, out))
code, out = call(owner, "create_box", {"name": "Odd", "tag": "a b@c"})
check("a tag with odd characters is refused", code != 200, (code, out))
ok(owner, "add_box_members", {"box_id": main, "users": [bob_id]})
check("the people of a box are listed", {m["name"] for m in ok(owner, "box_members", {"box_id": main})} >= {"owner", "bob"}, ok(owner, "box_members", {"box_id": main}))
check("bob sees the main box and not sales", [b["id"] for b in ok(bob, "my_boxes")] == [main], ok(bob, "my_boxes"))
check("cy sees sales boxes only", sales in [b["id"] for b in ok(cy, "my_boxes")] and main not in [b["id"] for b in ok(cy, "my_boxes")], ok(cy, "my_boxes"))

# --- mail arrives
subject = "Printer on fire " + RUN
code, out = post_mail(secret, mail(f"inbox@{DOMAIN}", subject, "The printer is on fire."))
check("mail to inbox@ is accepted", code == 200 and out["outcome"] == "routed", (code, out))


def find(b, text):
    return next((t for t in ok(b, "inbox") if text in t["subject"]), None)


t = wait(lambda: find(bob, subject))
check("the conversation shows up for the box's people", t is not None)
tid = t["id"]
check("it is open, from Ann, nobody has it", t["status"] == "open" and t["from"] == "Ann" and t["assignee"] == "—" and t["count"] == 1, t)
check("the owner (in the box) sees it", find(owner, subject) is not None)
check("cy (another box) does not", find(cy, subject) is None)
code, out = call(cy, "get_thread", {"id": tid})
check("cy cannot open it", code != 200, (code, out))
code, out = call(cy, "reply_thread", {"id": tid, "text": "hi"})
check("nor answer it", code != 200, (code, out))
g = ok(bob, "get_thread", {"id": tid})
check("the conversation shows the message", "The printer is on fire." in g["conversation"] and g["messages"] == 1, g)

# --- the same mail twice, a plus-tag box
m = mail(f"inbox@{DOMAIN}", "Dedupe " + RUN)
post_mail(secret, m)
code, dup = post_mail(secret, m)
check("a provider retry is a duplicate", dup.get("outcome") == "duplicate", dup)
sub2 = "Quote please " + RUN
post_mail(secret, mail(f"Inbox+{sales_tag}@{DOMAIN}", sub2, sender="dan@other.test", name="Dan"))
t2 = wait(lambda: find(cy, sub2))
check("mail to inbox+tag@ goes to the box with that tag", t2 is not None and find(bob, sub2) is None)
sub3 = "Where else " + RUN
post_mail(secret, mail(f"inbox+nosuch@{DOMAIN}", sub3))
check("an unknown tag falls back to the main box", wait(lambda: find(bob, sub3)) is not None)

# --- the customer answers: same conversation
post_mail(secret, mail(f"inbox@{DOMAIN}", f"Re: {subject} [#{tid}]", "It is out now, thanks."))
wait(lambda: next((t for t in ok(bob, "inbox") if t["id"] == tid and t["count"] == 2), None))
g = ok(bob, "get_thread", {"id": tid})
check("an answer with our [#key] joins the conversation", g["messages"] == 2 and "It is out now" in g["conversation"], g)
sub4 = "Re: Printer in-reply " + RUN
post_mail(secret, mail(f"inbox@{DOMAIN}", sub4, "second", **{"in_reply_to": f"<m1.{RUN}@client.test>"}))
wait(lambda: next((t for t in ok(bob, "inbox") if t["id"] == tid and t["count"] == 3), None))
check("an answer to a message of the conversation joins it too", ok(bob, "get_thread", {"id": tid})["messages"] == 3)

# --- working it
ok(bob, "take_thread", {"id": tid})
check("taking it shows who has it", ok(bob, "get_thread", {"id": tid})["assignee"] == "bob")
check("the 'mine' filter finds it", any(r["id"] == tid for r in ok(bob, "inbox", {"mine": True})) and not any(r["id"] == tid for r in ok(owner, "inbox", {"mine": True})))
code, out = call(bob, "assign_thread", {"id": tid, "user": cy_id})
check("it cannot go to someone outside the box", code != 200, (code, out))
ok(bob, "assign_thread", {"id": tid, "user": people["owner"] if "owner" in people else bob_id})
ok(bob, "release_thread", {"id": tid})
check("letting go clears it", ok(bob, "get_thread", {"id": tid})["assignee"] == "Nobody yet")

code, out = call(bob, "reply_thread", {"id": tid, "text": "  "})
check("an empty reply is refused", code != 200, (code, out))
code, out = call(bob, "reply_thread", {"id": tid, "text": "Glad it is sorted. Anything else?"})
before = ok(bob, "get_thread", {"id": tid})
if code == 200:
    g = ok(bob, "get_thread", {"id": tid})
    check("a reply is sent, kept in the conversation, as pending, taken by the sender", g["messages"] == 4 and g["status"] == "pending" and g["assignee"] == "bob" and "Glad it is sorted" in g["conversation"], g)
else:
    # this instance has no way to send mail (the settings of a mail bridge are not there): the reply must fail whole
    check("a reply that cannot be sent says why and is not kept", code == 422 and "could not be sent" in str(out) and before["messages"] == 3 and before["status"] == "open", (code, out, before))
    print("note: email sending is not set up in this instance; the sent-reply path is not covered here")

ok(bob, "close_thread", {"id": tid})
check("closing hides it from the default list", find(bob, subject) is None)
check("and shows under closed", any(r["id"] == tid for r in ok(bob, "inbox", {"status": "closed"})))
post_mail(secret, mail(f"inbox@{DOMAIN}", f"Re: {subject} [#{tid}]", "One more thing."))
back = wait(lambda: next((t for t in ok(bob, "inbox") if t["id"] == tid), None))
check("a new message from the customer opens a closed conversation again", back is not None and back["status"] == "open", back)
ok(bob, "close_thread", {"id": tid})
ok(bob, "reopen_thread", {"id": tid})
check("it can be reopened by hand", ok(bob, "get_thread", {"id": tid})["status"] == "open")

# --- searching
check("search finds by subject", any(r["id"] == tid for r in ok(bob, "inbox", {"q": "printer on fire"})))
check("search finds by sender", any(r["id"] == tid for r in ok(bob, "inbox", {"q": "ann@client"})))
check("filtering on a box", all(r["box"].startswith("Support") for r in ok(owner, "inbox", {"box": main})))

# --- the chatter of a conversation follows the box's people
code, ch = bob.call("GET", f"/api/chatter/{PLUGIN}/mail_thread/{tid}")
check("a member opens the conversation's chatter", code == 200, (code, ch))
code, ch = cy.call("GET", f"/api/chatter/{PLUGIN}/mail_thread/{tid}")
check("a stranger does not", code in (401, 403, 404), (code, ch))

# --- removing
code, out = call(bob, "delete_box", {"id": main})
check("an ordinary member cannot delete a box", code != 200, (code, out))
code, out = call(owner, "delete_box", {"id": main})
check("a box with conversations is not deleted", code != 200, (code, out))
mem = next(m for m in ok(owner, "box_members", {"box_id": main}) if m["name"] == "bob")
ok(owner, "remove_box_member", {"id": mem["id"]})
check("a person taken out no longer sees the box's mail", find(bob, subject) is None and main not in [b["id"] for b in ok(bob, "my_boxes")])
finish()
