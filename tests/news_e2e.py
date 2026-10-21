"""Atrium news, end to end over HTTP: who may write, drafts that others cannot see, publishing, the feed (pinned
first, search, categories, new marks), likes, read counts, comments through the kernel's chatter, and deleting."""
from e2e_lib import *

PLUGIN = "atrium_news"


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


owner, bob, cy = login(USER), login("bob"), login("cy")
people = {p["name"]: p["id"] for p in owner.call("POST", "/api/plugins/atrium_chat/list_people", {})[1]}
bob_id, cy_id = people["bob"], people["cy"]

# --- who may write
check("an administrator can write", ok(owner, "news_access")["can_post"] and ok(owner, "news_access")["is_admin"])
check("a member cannot, by default", not ok(bob, "news_access")["can_post"])
code, out = call(bob, "create_post", {"title": "Hello"})
check("so a member's post is refused", code != 200, (code, out))
code, out = call(bob, "add_authors", {"users": [bob_id]})
check("only an administrator names authors", code != 200, (code, out))
ok(owner, "add_authors", {"users": [bob_id]})
check("an author can write", ok(bob, "news_access")["can_post"])
check("authors are listed", any(a["id"] == bob_id for a in ok(cy, "authors")))
code, out = call(owner, "add_authors", {"users": ["users:nobody"]})
check("an author must be a member", code != 200, (code, out))

# --- drafts
draft = ok(bob, "create_post", {"title": "Q4 plans " + RUN, "summary": "What is next", "body": "We will **ship**.", "category": "Company"})["id"]
check("a new post is a draft", ok(bob, "get_post", {"id": draft})["status"] == "draft")
check("it is in the author's list", any(p["id"] == draft and p["tag"] == "Draft" for p in ok(bob, "my_posts")))
check("but not in the feed", all(p["id"] != draft for p in ok(cy, "feed")))
code, out = call(cy, "get_post", {"id": draft})
check("nobody else can open a draft", code != 200, (code, out))
check("an administrator can", call(owner, "get_post", {"id": draft})[0] == 200)
code, out = call(cy, "update_post", {"id": draft, "title": "x"})
check("nobody else can change it", code != 200, (code, out))
code, out = cy.call("GET", f"/api/chatter/{PLUGIN}/news_post/{draft}")
check("nor read its conversation", code != 200, (code, out))
code, out = call(bob, "publish_post", {"id": draft}) if False else (0, 0)
ok(bob, "update_post", {"id": draft, "title": "Q4 plans " + RUN, "body": "We will **ship** the plan."})
empty = ok(bob, "create_post", {"title": "Empty"})["id"]
code, out = call(bob, "publish_post", {"id": empty})
check("an empty post cannot be published", code != 200, (code, out))
ok(bob, "delete_post", {"id": empty})

# --- publishing
ok(bob, "publish_post", {"id": draft})
feed = ok(cy, "feed")
mine = next((p for p in feed if p["id"] == draft), None)
check("a published post is in everyone's feed", mine and mine["author"] == "bob" and mine["category"] == "Company", feed)
check("and marked new for someone who has not opened it", mine and mine["tag"] == "New", mine)
code, out = call(bob, "publish_post", {"id": draft})
check("it cannot be published twice", code != 200, (code, out))
post = ok(cy, "get_post", {"id": draft})
check("everyone can open it", post["title"].startswith("Q4 plans") and "ship" in post["rendered"] and not post["can_edit"], post)
check("after opening it is no longer new", next(p for p in ok(cy, "feed") if p["id"] == draft)["tag"] == "")
check("the author sees how many read it", ok(bob, "get_post", {"id": draft})["readers"] == 1)
check("and who", [r["name"] for r in ok(bob, "post_readers", {"id": draft})] == ["cy"])
code, out = call(cy, "post_readers", {"id": draft})
check("others do not", code != 200, (code, out))
check("a reader sees no count", ok(cy, "get_post", {"id": draft})["readers"] is None)
ok(cy, "get_post", {"id": draft})
check("opening twice counts once", ok(bob, "get_post", {"id": draft})["readers"] == 1)

# --- feed: search, categories, pinned first
second = ok(owner, "create_post", {"title": "Holiday hours " + RUN, "body": "Closed on Friday.", "category": "People", "publish": True})["id"]
pinned = ok(owner, "create_post", {"title": "Welcome " + RUN, "body": "Read this first", "pinned": True, "publish": True})["id"]
ids = [p["id"] for p in ok(cy, "feed")]
check("pinned posts come first, then the newest", ids.index(pinned) < ids.index(second) < ids.index(draft), ids)
check("a pinned post is tagged", next(p for p in ok(cy, "feed") if p["id"] == pinned)["tag"] == "Pinned")
check("the feed is searched", [p["id"] for p in ok(cy, "feed", {"q": "closed on friday"})] == [second])
check("and filtered by category", {p["id"] for p in ok(cy, "feed", {"category": "company"})} >= {draft} and second not in {p["id"] for p in ok(cy, "feed", {"category": "company"})})
check("categories are listed", {c["name"] for c in ok(cy, "category_options")} >= {"Company", "People"})
ok(owner, "pin_post", {"id": pinned, "pinned": False})
check("a post can be unpinned", next(p for p in ok(cy, "feed") if p["id"] == pinned)["tag"] != "Pinned")
code, out = call(bob, "pin_post", {"id": pinned, "pinned": True})
check("only its author or an administrator pins", code != 200, (code, out))

# --- likes
check("a like is counted", ok(cy, "toggle_like", {"id": draft}) == {"liked": True, "likes": 1})
ok(bob, "toggle_like", {"id": draft})
check("another like adds", ok(cy, "get_post", {"id": draft})["likes"] == 2 and ok(cy, "get_post", {"id": draft})["liked"])
check("a second press takes it back", ok(cy, "toggle_like", {"id": draft}) == {"liked": False, "likes": 1})
check("the feed shows likes", next(p for p in ok(cy, "feed") if p["id"] == draft)["likes"] == 1)

# --- comments
code, out = cy.call("GET", f"/api/chatter/{PLUGIN}/news_post/{draft}")
check("anyone reads the conversation under a published post", code == 200, (code, str(out)[:150]))
code, out = cy.call("POST", f"/api/chatter/{PLUGIN}/news_post/{draft}/messages", {"kind": "message", "body": "Great news"})
check("and comments", code in (200, 201), (code, str(out)[:150]))

# --- editing rights, back to draft, deleting
code, out = call(cy, "update_post", {"id": draft, "title": "hijack"})
check("a reader cannot edit a published post", code != 200, (code, out))
ok(owner, "update_post", {"id": draft, "summary": "Edited by an administrator"})
check("an administrator can", ok(cy, "get_post", {"id": draft})["summary"] == "Edited by an administrator")
ok(bob, "unpublish_post", {"id": draft})
check("an author takes a post back to a draft", all(p["id"] != draft for p in ok(cy, "feed")))
code, out = call(cy, "toggle_like", {"id": draft})
check("a draft cannot be liked", code != 200, (code, out))
ok(bob, "publish_post", {"id": draft, "notify": False})
code, out = call(cy, "delete_post", {"id": draft})
check("a reader cannot delete", code != 200, (code, out))
ok(bob, "delete_post", {"id": draft})
check("an author deletes their post", call(cy, "get_post", {"id": draft})[0] != 200)
ok(owner, "delete_post", {"id": second})
ok(owner, "delete_post", {"id": pinned})

# --- taking authors away
ok(owner, "remove_author", {"id": bob_id})
check("a removed author can no longer write", call(bob, "create_post", {"title": "again"})[0] != 200)

finish()
