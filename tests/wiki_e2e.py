"""Atrium wiki, end to end over HTTP: spaces and their roles, open spaces, pages in a tree, links between pages,
the past of a page and putting a version back, search, and that strangers see nothing."""
from e2e_lib import *

PLUGIN = "atrium_wiki"


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
owner_id = {p["name"]: p["id"] for p in bob.call("POST", "/api/plugins/atrium_chat/list_people", {})[1]}["owner"]

# --- spaces
sp = ok(owner, "create_space", {"name": "Handbook " + RUN, "description": "How we work", "members": [bob_id]})["id"]
check("a space is made", any(s["id"] == sp and s["role"] == "owner" and s["people"] == 2 for s in ok(owner, "my_spaces")))
check("a person added to it sees it, as an editor", any(s["id"] == sp and s["role"] == "editor" for s in ok(bob, "my_spaces")))
check("a stranger does not", all(s["id"] != sp for s in ok(cy, "my_spaces")))
code, out = call(cy, "space_info", {"space": sp})
check("a stranger cannot open it", code != 200, (code, out))
code, out = call(cy, "space_pages", {"space": sp})
check("nor list its pages", code != 200, (code, out))
code, out = call(owner, "create_space", {"name": "  "})
check("a space needs a name", code != 200, (code, out))
check("spaces can be searched", any(s["id"] == sp for s in ok(owner, "my_spaces", {"q": "handbook " + RUN})) and not ok(owner, "my_spaces", {"q": "zzzz-none"}))

# --- pages
home = ok(owner, "create_page", {"space": sp, "title": "Welcome", "body": "# Welcome\n\nStart with [[Onboarding]] and the [[Expense policy|policy]]."})["id"]
onb = ok(bob, "create_page", {"space": sp, "title": "Onboarding", "body": "First day checklist.", "parent": home})["id"]
sub = ok(bob, "create_page", {"space": sp, "title": "Laptops", "body": "Order via IT.", "parent": onb})["id"]
code, out = call(cy, "create_page", {"space": sp, "title": "Hack"})
check("a stranger cannot write", code != 200, (code, out))
code, out = call(owner, "create_page", {"space": sp, "title": "welcome"})
check("a title is unique in its space, ignoring case", code != 200, (code, out))
code, out = call(owner, "create_page", {"space": sp, "title": "Bad [[title]]"})
check("a title cannot hold brackets", code != 200, (code, out))
code, out = call(owner, "create_page", {"space": sp, "title": "Orphan", "parent": "nonexistent"})
check("a parent must be a page of the space", code != 200, (code, out))
tree = ok(owner, "space_pages", {"space": sp})
check("the tree is in reading order with depth", [(t["title"], t["depth"]) for t in tree] == [("Welcome", 0), ("Onboarding", 1), ("Laptops", 2)], tree)
check("the tree shows who and when", all(t["by"] and t["edited"] for t in tree), tree)
page = ok(owner, "get_page", {"id": home})
check("a page's links lead to pages of the space", f"[Onboarding](/wiki/p/{onb})" in page["rendered"], page["rendered"])
check("an unknown title stays as typed", "[[Expense policy|policy]]" in page["rendered"], page["rendered"])
check("subpages are listed on the page", f"[Onboarding](/wiki/p/{onb})" in page["rendered"].split("Pages under this one")[-1], page["rendered"])
check("the page says where it is", ok(owner, "get_page", {"id": sub})["path"] == "Handbook " + RUN + " › Welcome › Onboarding", ok(owner, "get_page", {"id": sub}))
check("and what links to it", ok(owner, "get_page", {"id": onb})["linked_from"] == "Welcome", ok(owner, "get_page", {"id": onb}))
check("the editing form has the markdown", ok(owner, "get_page", {"id": home})["body"].startswith("# Welcome"))
code, out = call(cy, "get_page", {"id": home})
check("a stranger cannot read a page", code != 200, (code, out))

# --- the record rule: readers talk about a page through the kernel's chatter, strangers cannot
code, out = bob.call("GET", f"/api/chatter/{PLUGIN}/wiki_page/{home}")
check("a member reads a page's conversation", code == 200, (code, out))
code, out = bob.call("POST", f"/api/chatter/{PLUGIN}/wiki_page/{home}/messages", {"kind": "message", "body": "nice page"})
check("and comments on it", code in (200, 201), (code, out))
code, out = cy.call("GET", f"/api/chatter/{PLUGIN}/wiki_page/{home}")
check("a stranger does not", code != 200, (code, out))
code, out = cy.call("POST", f"/api/chatter/{PLUGIN}/wiki_page/{home}/messages", {"kind": "message", "body": "hi"})
check("nor comments", code not in (200, 201), (code, out))

# --- changing, moving
ok(bob, "update_page", {"id": home, "body": "# Welcome\n\nNew text about [[Laptops]]."})
check("an editor changes a page", "[Laptops]" in ok(owner, "get_page", {"id": home})["rendered"])
code, out = call(owner, "update_page", {"id": onb, "parent": sub})
check("a page cannot go under its own subpage", code != 200, (code, out))
code, out = call(owner, "update_page", {"id": onb, "parent": onb})
check("nor under itself", code != 200, (code, out))
picks = ok(owner, "pick_pages", {"id": onb})
check("the parent picker leaves out the page and what is below it", [p["name"] for p in picks] == ["Welcome"], picks)
ok(owner, "update_page", {"id": sub, "parent": ""})
check("a page can go to the top", [(t["title"], t["depth"]) for t in ok(owner, "space_pages", {"space": sp})] == [("Laptops", 0), ("Welcome", 0), ("Onboarding", 1)])
code, out = call(owner, "update_page", {"id": sub, "title": "Welcome"})
check("a rename cannot clash", code != 200, (code, out))
ok(owner, "update_page", {"id": sub, "title": "Laptops and phones"})

# --- history
versions = ok(owner, "page_history", {"id": home})
check("every change is a version", len(versions) >= 2 and all(v["who"] for v in versions), versions)
first = [v for v in versions if v["op"] in ("create", "baseline")][-1]
check("a version says what changed", any("body" in v["changed"] for v in versions if v["op"] == "update"), versions)
code, out = call(cy, "page_history", {"id": home})
check("a stranger gets no history", code != 200 or out == [], (code, out))
ok(owner, "restore_version", {"id": first["id"]})
check("an earlier version can be put back", "Start with" in ok(owner, "get_page", {"id": home})["body"])
check("and that is a new version", len(ok(owner, "page_history", {"id": home})) > len(versions))

# --- search
found = ok(bob, "search_pages", {"q": "checklist"})
check("search finds text inside pages", any(f["id"] == onb and f["found"] == "text" and "checklist" in f["snippet"].lower() for f in found), found)
check("title matches come first", ok(bob, "search_pages", {"q": "welcome"})[0]["id"] == home)
check("a stranger finds nothing of it", all(f["id"] not in (home, onb) for f in ok(cy, "search_pages", {"q": "checklist"})))
check("one letter is not a search", ok(bob, "search_pages", {"q": "a"}) == [])
check("recent changes list the page", any(r["id"] == home for r in ok(owner, "recent_pages")))
check("and not for a stranger", all(r["id"] != home for r in ok(cy, "recent_pages")))

# --- deleting a page lifts its subpages
ok(owner, "delete_page", {"id": home})
check("deleting a page lifts the pages under it", [(t["title"], t["depth"]) for t in ok(owner, "space_pages", {"space": sp})] == [("Laptops and phones", 0), ("Onboarding", 0)])

# --- people and roles
code, out = call(bob, "add_space_members", {"space": sp, "users": [cy_id]})
check("only an owner adds people", code != 200, (code, out))
ok(owner, "add_space_members", {"space": sp, "users": [cy_id], "role": "reader"})
check("a reader sees the space", any(s["id"] == sp and s["role"] == "reader" for s in ok(cy, "my_spaces")))
check("and its pages", len(ok(cy, "space_pages", {"space": sp})) == 2)
code, out = call(cy, "create_page", {"space": sp, "title": "Hack"})
check("a reader cannot write", code != 200, (code, out))
code, out = call(cy, "update_page", {"id": onb, "body": "x"})
check("nor change", code != 200, (code, out))
code, out = call(cy, "delete_page", {"id": onb})
check("nor delete", code != 200, (code, out))
code, out = call(owner, "set_space_role", {"space": sp, "id": owner_id, "role": "reader"})
check("a space keeps an owner", code != 200, (code, out))
code, out = call(owner, "remove_space_member", {"space": sp, "id": owner_id})
check("the last owner cannot leave", code != 200, (code, out))
ok(owner, "set_space_role", {"space": sp, "id": cy_id, "role": "editor"})
ok(cy, "create_page", {"space": sp, "title": "By cy"})
ok(cy, "remove_space_member", {"space": sp, "id": cy_id})
check("anyone may leave", all(s["id"] != sp for s in ok(cy, "my_spaces")))

# --- open space
ok(owner, "edit_space", {"space": sp, "open": True})
check("an open space is readable by everyone", any(s["id"] == sp and s["role"] == "reader" and not s["member"] for s in ok(cy, "my_spaces")))
check("with its pages", len(ok(cy, "space_pages", {"space": sp})) == 3)
code, out = call(cy, "create_page", {"space": sp, "title": "Still not"})
check("but not writable", code != 200, (code, out))
check("and searchable", any(f["id"] == onb for f in ok(cy, "search_pages", {"q": "checklist"})))
ok(owner, "edit_space", {"space": sp, "open": False})
check("closing it hides it again", all(s["id"] != sp for s in ok(cy, "my_spaces")))

# --- deleting a space
code, out = call(bob, "delete_space", {"space": sp})
check("only an owner deletes a space", code != 200, (code, out))
ok(owner, "delete_space", {"space": sp})
check("a deleted space is gone", all(s["id"] != sp for s in ok(owner, "my_spaces")))
check("with its pages", call(owner, "get_page", {"id": onb})[0] != 200)

finish()
