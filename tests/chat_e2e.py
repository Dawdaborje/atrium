"""Atrium chat, end to end over HTTP and a real WebSocket. See README.md for the one-time setup.

Needs `websocket-client`. Makes its own channels each run, so it can be repeated. Exits 1 when a check fails."""
import json, time
import websocket
from e2e_lib import *

WS = BASE.replace("http://", "ws://") + "/api/ui/realtime"
PLUGIN = "atrium_chat"


def login(user):
    b = Browser()
    b.call("GET", "/api/auth/methods")
    code, body = b.call("POST", "/api/auth/login", {"username": user, "password": PASSWORD})
    assert code == 200, (user, code, body)
    return b


def call(b, fn, body=None, expect=200):
    code, out = b.call("POST", f"/api/plugins/{PLUGIN}/{fn}", body or {})
    return code, out


def ok(b, fn, body=None):
    code, out = call(b, fn, body)
    assert code == 200, (fn, code, out)
    return out


def socket(b):
    cookie = "; ".join(f"{c.name}={c.value}" for c in b.jar)
    return websocket.create_connection(WS, header=["Cookie: " + cookie], origin=BASE, timeout=3)


def frames(ws, wait=0.5):
    out, end = [], time.time() + wait
    while time.time() < end:
        ws.settimeout(max(0.05, end - time.time()))
        try:
            data = ws.recv()
            if not data:
                out.append({"t": "closed"})
                break
            out.append(json.loads(data))
        except websocket.WebSocketTimeoutException:
            break
    return out


def of(fs, t):
    return [f for f in fs if f.get("t") == t]


def sub(ws, topic, since=None):
    ws.send(json.dumps({"op": "sub", "topic": topic, **({"since": since} if since is not None else {})}))
    r = of(frames(ws), "sub")
    return r[0] if r else {}


owner, bob, cy = login(USER), login("bob"), login("cy")
people = {p["name"]: p["id"] for p in ok(owner, "list_people")}
bob_id, cy_id = people.get("bob"), people.get("cy")
check("the people picker lists everyone but the caller", bob_id and cy_id and "owner" not in people, people)
key = lambda i: i.split(":", 1)[-1]

# --- the starter channel and browsing
browse = ok(bob, "browse_channels")
general = next((c for c in browse if c["name"] == "general"), None)
check("the starter channel exists and is open to join", general is not None, browse)

# --- teams and channels
code, out = call(bob, "create_team", {"name": "Nope " + RUN})
check("an ordinary member cannot make a team", code != 200, (code, out))
team = ok(owner, "create_team", {"name": "Ops " + RUN})["id"]
code, out = call(owner, "create_team", {"name": "Ops " + RUN})
check("a team name is unique", code != 200, (code, out))
ch = ok(owner, "create_channel", {"name": "alerts-" + RUN, "team": team, "topic": "Things on fire"})["id"]
code, out = call(owner, "create_channel", {"name": "alerts-" + RUN, "team": team})
check("a channel name is unique inside its team", code != 200, (code, out))
rail = ok(owner, "my_channels")
mine = next((c for c in rail if c["id"] == ch), None)
check("the creator sees it in the rail under its team", mine and mine["group"] == "Ops " + RUN and mine["unread"] == 0, rail)
check("bob does not see it before joining", all(c["id"] != ch for c in ok(bob, "my_channels")))
check("bob can find it by browsing", any(c["id"] == ch for c in ok(bob, "browse_channels")))

# --- live delivery on a public channel, and who may join
topic = f"{PLUGIN}:channel.{ch}"
wa, wb, wc = socket(owner), socket(bob), socket(cy)
check("the owner joins the channel's topic", sub(wa, topic).get("ok") is True)
r = sub(wb, topic)
check("bob may read a public channel without joining it", r.get("ok") is True, r)
check("the presence list names both", len(r.get("presence", [])) == 2, r)
posted = ok(bob, "post_message", {"channel": ch, "body": "hello team"})
check("posting in a public channel joins the poster", any(c["id"] == ch for c in ok(bob, "my_channels")))
fa = frames(wa)
msg = of(fa, "msg")
check("the owner receives it live with the row", msg and msg[0]["payload"]["row"]["body"] == "hello team" and msg[0]["payload"]["row"]["n"] == 1, fa)
check("the row carries the author's display name", msg and msg[0]["payload"]["row"]["author_name"] == "bob", msg)
check("the author is not unread on their own message", next(c for c in ok(bob, "my_channels") if c["id"] == ch)["unread"] == 0)
check("the other member has it unread", next(c for c in ok(owner, "my_channels") if c["id"] == ch)["unread"] == 1)
page = ok(owner, "channel_messages", {"channel": ch})
check("opening the channel returns the message and clears unread", [r["body"] for r in page["rows"]] == ["hello team"] and next(c for c in ok(owner, "my_channels") if c["id"] == ch)["unread"] == 0, page)

# --- validation
for body, label in [({"channel": ch}, "an empty message"), ({"channel": ch, "body": "x" * 8001}, "a message over 8000 characters"), ({"channel": ch, "body": "hi", "files": [{"key": "plugins/x", "name": "a"}]}, "a file that is not from the media store"), ({"channel": ch, "body": "hi", "files": [{"key": "media/../x", "name": "a"}]}, "a path that climbs")]:
    code, out = call(owner, "post_message", body)
    check(f"{label} is refused", code != 200, (code, out))
files = [{"key": "media/2026/10/ab.txt", "name": "note.txt", "mime": "text/plain", "size": 12}]
withfile = ok(owner, "post_message", {"channel": ch, "body": "see file", "files": files})
check("a file from the media store is kept on the message", withfile["row"]["files"][0]["name"] == "note.txt", withfile)

# --- typing
wb.send(json.dumps({"op": "event", "topic": topic, "kind": "typing"}))
check("typing reaches the others", of(frames(wa), "event") != [])

# --- private channels, adding and removing people
private = ok(owner, "create_channel", {"name": "board-" + RUN, "private": True})["id"]
ptopic = f"{PLUGIN}:channel.{private}"
r = sub(wc, ptopic)
check("a private channel refuses a non-member's socket", r.get("ok") is False, r)
code, out = call(cy, "channel_messages", {"channel": private})
check("...and refuses them its messages", code != 200, (code, out))
code, out = call(cy, "post_message", {"channel": private, "body": "let me in"})
check("...and refuses their posts", code != 200, (code, out))
code, out = call(cy, "join_channel", {"channel": private})
check("...and refuses a join", code != 200, (code, out))
code, out = call(bob, "add_members", {"channel": private, "users": [bob_id]})
check("only the owner adds people to a channel", code != 200, (code, out))
ok(owner, "add_members", {"channel": private, "users": [cy_id]})
check("once added, they may join its topic", sub(wc, ptopic).get("ok") is True)
ok(owner, "remove_member", {"channel": private, "user": cy_id})
check("removing a person drops them from the live topic", of(frames(wc), "dropped") != [])
code, out = call(cy, "channel_messages", {"channel": private})
check("...and from its messages", code != 200, (code, out))

# --- direct messages
d1 = ok(owner, "open_direct", {"user": bob_id})["id"]
rail_o, rail_b = ok(owner, "my_channels"), ok(bob, "my_channels")
dm_o = next((c for c in rail_o if c["id"] == d1), None)
dm_b = next((c for c in rail_b if c["id"] == d1), None)
check("a direct message shows the other person's name", dm_o and dm_o["name"] == "bob" and dm_b and dm_b["name"] == "owner", (dm_o, dm_b))
check("...in its own group, with the person for the online dot", dm_o["group"] == "Direct messages" and dm_o["user"], dm_o)
check("opening the same direct message again finds the same chat", ok(owner, "open_direct", {"user": bob_id})["id"] == d1)
code, out = call(owner, "open_direct", {"user": "nobody"})
check("a direct message to a stranger is refused", code != 200, (code, out))
ok(owner, "post_message", {"channel": d1, "body": "psst"})
code, out = call(cy, "channel_messages", {"channel": d1})
check("a third person cannot read a direct message", code != 200, (code, out))
code, out = call(cy, "post_message", {"channel": d1, "body": "hi"})
check("...or write in it", code != 200, (code, out))
code, out = call(owner, "leave_channel", {"channel": d1})
check("a direct message cannot be left", code != 200, (code, out))

# --- group chats
code, out = call(owner, "create_group", {"members": [bob_id]})
check("a group needs two other people", code != 200, (code, out))
g = ok(owner, "create_group", {"members": [bob_id, cy_id]})["id"]
rg = next(c for c in ok(cy, "my_channels") if c["id"] == g)
check("a group without a name is named after the others", set(rg["name"].split(", ")) == {"owner", "bob"} and rg["group"] == "Groups", rg)
ok(cy, "post_message", {"channel": g, "body": "group hello"})
check("every member sees the group message", [r["body"] for r in ok(bob, "channel_messages", {"channel": g})["rows"]] == ["group hello"])

# --- threads
root = posted["id"]
tw = socket(bob)
check("a thread's topic opens for someone who can read the channel", sub(tw, f"{PLUGIN}:thread.{root}").get("ok") is True)
reply = ok(owner, "post_message", {"channel": ch, "body": "on it", "query": {"thread": root}})
check("a reply is stored under its message", reply["row"]["parent"] == root, reply)
ft = frames(tw)
check("the thread's watchers get the reply live", [m["payload"]["row"]["body"] for m in of(ft, "msg")] == ["on it"], ft)
fa = frames(wa)
parents = [m["payload"]["row"] for m in of(fa, "msg") if m["payload"]["row"].get("id", "").endswith(root)]
check("the channel is told the message now has a reply", parents and parents[-1]["reply_count"] == 1, fa)
check("a reply is not a message of the channel itself", all(r["id"].split(":")[-1] != reply["id"] for r in ok(owner, "channel_messages", {"channel": ch})["rows"]))
th = ok(bob, "thread_messages", {"thread": root})
check("a thread lists its message first, then the replies", [r["body"] for r in th["rows"]] == ["hello team", "on it"], th)
code, out = call(owner, "post_message", {"channel": ch, "body": "nested", "query": {"thread": reply["id"]}})
check("threads are one level deep", code != 200, (code, out))
code, out = call(owner, "post_message", {"channel": d1, "body": "wrong place", "parent": root})
check("a reply must be in the message's own channel", code != 200, (code, out))
code, out = call(cy, "thread_messages", {"thread": ok(owner, "post_message", {"channel": d1, "body": "in dm"})["id"]})
check("a thread inside a direct message stays private", code != 200, (code, out))
check("a stranger cannot open the thread topic of a private conversation", sub(socket(cy), f"{PLUGIN}:thread.{ok(owner, 'post_message', {'channel': d1, 'body': 'x'})['id']}").get("ok") is False)

# --- editing and deleting
mine_id = ok(owner, "post_message", {"channel": ch, "body": "typo"})["id"]
frames(wa); frames(wb)
edited = ok(owner, "edit_message", {"id": mine_id, "body": "fixed"})
check("an author edits their message", edited["row"]["body"] == "fixed" and edited["row"].get("edited_at"), edited)
fb = frames(wb)
check("the edit reaches the channel live as a replacement", any(m["payload"]["row"]["body"] == "fixed" for m in of(fb, "msg")), fb)
code, out = call(bob, "edit_message", {"id": mine_id, "body": "hijack"})
check("nobody else edits it", code != 200, (code, out))
code, out = call(bob, "delete_message", {"id": mine_id})
check("an ordinary member cannot delete someone else's message", code != 200, (code, out))
ok(owner, "delete_message", {"id": mine_id})
fb = frames(wb)
check("a deletion reaches the channel as a `deleted` event", any(m["event"] == "deleted" for m in of(fb, "msg")), fb)
check("a deleted message is no longer listed", all(not r["id"].endswith(mine_id) for r in ok(owner, "channel_messages", {"channel": ch})["rows"]))
code, out = call(owner, "edit_message", {"id": mine_id, "body": "again"})
check("a deleted message cannot be edited", code != 200, (code, out))
bobs = ok(bob, "post_message", {"channel": ch, "body": "mine"})["id"]
ok(owner, "delete_message", {"id": bobs})
check("a channel owner can delete a message of another", True)

# --- inbox and online topics
btopic = f"{PLUGIN}:inbox.{key(bob_id)}"
wi = socket(bob)
check("a person joins their own inbox", sub(wi, btopic).get("ok") is True)
r = sub(socket(owner), btopic)
check("nobody joins someone else's inbox", r.get("ok") is False, r)
ok(owner, "post_message", {"channel": ch, "body": "ping for bob"})
fi = frames(wi)
check("a message nudges each member's inbox with the channel", [m["payload"].get("channel") for m in of(fi, "msg")] == [ch], fi)
check("everyone may join the online topic", sub(socket(cy), f"{PLUGIN}:online").get("ok") is True)

# --- archive and edit
ok(owner, "edit_channel", {"channel": ch, "topic": "New topic"})
code, out = call(bob, "edit_channel", {"channel": ch, "name": "mine now"})
check("only an owner renames a channel", code != 200, (code, out))
ok(owner, "edit_channel", {"channel": ch, "archived": True})
code, out = call(owner, "post_message", {"channel": ch, "body": "too late"})
check("an archived channel takes no new messages", code != 200, (code, out))
check("...and leaves the rail", all(c["id"] != ch for c in ok(owner, "my_channels")))
info = ok(owner, "channel_info", {"channel": private})
check("channel info lists the people and the caller's role", info["role"] == "owner" and len(info["members"]) == 1, info)


# --- reactions
rid = ok(owner, "post_message", {"channel": g, "body": "react to me"})["id"]
frames(wb)
r1 = ok(bob, "react", {"id": rid, "emoji": "👍"})["row"]
check("a reaction is added with the person who made it", list(r1["reactions"]) == ["👍"] and len(r1["reactions"]["👍"]) == 1, r1)
r2 = ok(cy, "react", {"id": rid, "emoji": "👍"})["row"]
check("a second person joins the same reaction", len(r2["reactions"]["👍"]) == 2, r2)
r3 = ok(bob, "react", {"id": rid, "emoji": "👍"})["row"]
check("reacting again takes it back", len(r3["reactions"]["👍"]) == 1, r3)
r4 = ok(cy, "react", {"id": rid, "emoji": "👍"})["row"]
check("the last reaction removes the emoji", r4["reactions"] == {}, r4)
for bad in ["", "two words", "x" * 20]:
    code, out = call(bob, "react", {"id": rid, "emoji": bad})
    check(f"the reaction {bad[:10]!r} is refused", code != 200, (code, out))
code, out = call(cy, "react", {"id": ok(owner, "post_message", {"channel": d1, "body": "dm"})["id"], "emoji": "👍"})
check("a stranger cannot react in a direct message", code != 200, (code, out))
gw = socket(owner)
sub(gw, f"{PLUGIN}:channel.{g}")
ok(bob, "react", {"id": rid, "emoji": "🎉"})
fg = frames(gw)
check("a reaction reaches the channel live as the changed row", any(m["payload"]["row"].get("reactions", {}).get("🎉") for m in of(fg, "msg")), fg)

# --- mentions
ownname = "owner"
m1 = ok(bob, "post_message", {"channel": g, "body": f"hello @{ownname} and @cy and @nobody"})
check("mentioned people are recorded by id and name", len(m1["row"]["mentions"]) == 2 and set(m1["row"]["mention_names"]) == {ownname, "cy"}, m1)
rail_o = next(c for c in ok(owner, "my_channels") if c["id"] == g)
check("a mention marks the channel in the rail for the person mentioned", rail_o["mentions"] == 1, rail_o)
rail_b = next(c for c in ok(bob, "my_channels") if c["id"] == g)
check("...but not for the author", rail_b["mentions"] == 0, rail_b)
ok(owner, "channel_messages", {"channel": g})
rail_o = next(c for c in ok(owner, "my_channels") if c["id"] == g)
check("reading the channel clears the mention", rail_o["mentions"] == 0 and rail_o["unread"] == 0, rail_o)
m2 = ok(bob, "post_message", {"channel": g, "body": "attention @channel"})
check("@channel mentions everyone else in the channel", len(m2["row"]["mentions"]) == 2, m2)
ok(owner, "mark_read", {"channel": g})
check("marking a channel read clears its mentions", next(c for c in ok(owner, "my_channels") if c["id"] == g)["mentions"] == 0)
priv2 = ok(owner, "create_channel", {"name": "quiet-" + RUN, "private": True})["id"]
m3 = ok(owner, "post_message", {"channel": priv2, "body": f"psst @bob"})
check("someone who cannot read a private channel is not mentioned in it", m3["row"]["mentions"] == [], m3)

# --- search
ok(owner, "post_message", {"channel": g, "body": "needle in a haystack " + RUN})
found = ok(bob, "search_messages", {"q": "needle in a haystack " + RUN})
check("search finds a message in a conversation the person is in", found["total"] == 1 and found["rows"][0]["channel"] == g, found)
check("...with the conversation's name", bool(found["rows"][0]["channel_name"]), found)
dmsec = ok(owner, "post_message", {"channel": d1, "body": "secret needle " + RUN})
check("search does not reach a direct message of others", ok(cy, "search_messages", {"q": "secret needle " + RUN})["total"] == 0)
check("a one-letter search returns nothing", ok(owner, "search_messages", {"q": "a"})["total"] == 0)
open3 = ok(owner, "create_channel", {"name": "open-" + RUN})["id"]
ok(owner, "post_message", {"channel": open3, "body": "visible to all " + RUN})
check("search finds an open channel's message without joining", ok(cy, "search_messages", {"q": "visible to all " + RUN})["total"] == 1)

# --- members and header
members = ok(owner, "channel_members", {"channel": g})
check("the members list names each person with a role", len(members) == 3 and all(m["name"] and m["role"] for m in members), members)
head = ok(owner, "channel_header", {"channel": ch})
check("the header gives a title and a subtitle", head["title"] == "# alerts-" + RUN and "people" in head["subtitle"] or "person" in head["subtitle"], head)
hdm = ok(owner, "channel_header", {"channel": d1})
check("a direct message's title is the other person", hdm["title"] == "bob" and hdm["subtitle"] == "Direct message", hdm)
ok(owner, "remove_member", {"channel": g, "id": key(cy_id)})
check("a member can be removed by the key a list row sends", len(ok(owner, "channel_members", {"channel": g})) == 2)
ok(bob, "set_muted", {"channel": g, "muted": True})
check("a person mutes a conversation for themselves", next(c for c in ok(bob, "my_channels") if c["id"] == g)["muted"] is True)

# --- pinned messages and alerts
pin_msg = ok(owner, "post_message", {"channel": open3, "body": "pin me " + RUN})["row"]["id"].split(":")[-1]
check("a message starts unpinned", ok(owner, "pinned_messages", {"channel": open3}) == [])
code, out = call(cy, "pin_message", {"id": pin_msg})
check("a stranger to the channel cannot pin", code != 200, (code, out))
pinned = ok(owner, "pin_message", {"id": pin_msg})
check("a member pins a message", pinned["pinned"] is True and pinned["row"]["pinned"] is True, pinned)
rows = ok(owner, "pinned_messages", {"channel": open3})
check("it is on the pinned page with who wrote it", len(rows) == 1 and rows[0]["id"] == pin_msg and rows[0]["author_name"] and "pin me" in rows[0]["body"], rows)
check("the pin shows on the message in the channel", any(r["id"].split(":")[-1] == pin_msg and r.get("pinned") for r in ok(owner, "channel_messages", {"channel": open3})["rows"]))
check("pinning again takes the pin off", ok(owner, "pin_message", {"id": pin_msg})["pinned"] is False and ok(owner, "pinned_messages", {"channel": open3}) == [])
ok(owner, "pin_message", {"id": pin_msg})
ok(owner, "delete_message", {"id": pin_msg})
check("a deleted message is no longer pinned", ok(owner, "pinned_messages", {"channel": open3}) == [])
mine = ok(bob, "my_membership", {"channel": g})
check("a person reads their own alert settings", mine["muted"] is True, mine)
ok(bob, "set_muted", {"channel": g, "muted": False})
check("and changes them", ok(bob, "my_membership", {"channel": g})["muted"] is False)
code, out = call(cy, "my_membership", {"channel": g})
check("a person outside the conversation has no settings in it", code != 200, (code, out))

# --- for other plugins
code, out = call(bob, "ensure_channel", {"name": "hr-" + RUN})
check("an ordinary member cannot use the service functions", code != 200, (code, out))
ens = ok(owner, "ensure_channel", {"name": "dept-" + RUN, "team": "Departments", "members": [bob_id], "topic": "From HR"})
check("an administrator ensures a channel in a team, made on the way", ens["created"] is True and ens["added"] == 1, ens)
ens2 = ok(owner, "ensure_channel", {"name": "dept-" + RUN, "team": "Departments", "members": [bob_id, cy_id]})
check("ensuring it again finds it and adds only the new person", ens2["created"] is False and ens2["id"] == ens["id"] and ens2["added"] == 1, ens2)
syn = ok(owner, "sync_members", {"channel": ens["id"], "users": [cy_id], "remove_others": True})
check("syncing makes the members exactly the list (the owner stays)", syn["removed"] == 1 and syn["added"] == 0, syn)
gone = ok(owner, "remove_everywhere", {"user": cy_id})
check("a person who left is removed from every shared conversation", gone["removed"] >= 1, gone)

finish()
