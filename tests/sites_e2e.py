"""Atrium sites, end to end over HTTP: sites and their roles, open sites, the start page with links and (when installed)
the latest news, link checks and order, and that strangers see nothing."""
from e2e_lib import *

PLUGIN = "atrium_sites"


def login(user):
    b = Browser()
    b.call("GET", "/api/auth/methods")
    code, body = b.call("POST", "/api/auth/login", {"username": user, "password": PASSWORD})
    assert code == 200, (user, code, body)
    return b


def call(b, fn, body=None, plugin=PLUGIN):
    return b.call("POST", f"/api/plugins/{plugin}/{fn}", body or {})


def ok(b, fn, body=None, plugin=PLUGIN):
    code, out = call(b, fn, body, plugin)
    assert code == 200, (fn, code, out)
    return out


owner, bob, cy = login(USER), login("bob"), login("cy")
people = {p["name"]: p["id"] for p in owner.call("POST", "/api/plugins/atrium_chat/list_people", {})[1]}
bob_id, cy_id = people["bob"], people["cy"]
owner_id = {p["name"]: p["id"] for p in bob.call("POST", "/api/plugins/atrium_chat/list_people", {})[1]}["owner"]

# --- sites
s = ok(owner, "create_site", {"name": "Sales " + RUN, "description": "The sales team", "home": "# Welcome\n\nWe sell things.", "members": [bob_id]})["id"]
check("a site is made", any(x["id"] == s and x["role"] == "owner" and x["people"] == 2 for x in ok(owner, "my_sites")))
check("an added person is an editor", any(x["id"] == s and x["role"] == "editor" for x in ok(bob, "my_sites")))
check("a stranger does not see it", all(x["id"] != s for x in ok(cy, "my_sites")))
code, out = call(cy, "view_site", {"site": s})
check("nor open it", code != 200, (code, out))
code, out = call(cy, "site_links", {"site": s})
check("nor its links", code != 200, (code, out))
code, out = call(owner, "create_site", {"name": " "})
check("a site needs a name", code != 200, (code, out))
check("sites can be searched", [x["id"] for x in ok(owner, "my_sites", {"q": "sales " + RUN})] == [s] and not ok(owner, "my_sites", {"q": "zzzz-none"}))

# --- the start page
page = ok(owner, "view_site", {"site": s})
check("the start page has the description and the words", page["page"].startswith("*The sales team*") and "We sell things." in page["page"], page)
check("without links there is no link section", "Quick links" not in page["page"])

# --- links
code, out = call(bob, "add_link", {"site": s, "title": "Pipeline", "url": "/crm", "about": "Open deals"})
check("an editor adds a link", code == 200, (code, out))
l1 = out["id"]
l2 = ok(bob, "add_link", {"site": s, "title": "Handbook", "url": "https://example.org/handbook"})["id"]
l3 = ok(owner, "add_link", {"site": s, "title": "Wiki", "url": "/wiki"})["id"]
for bad in ("//evil.example/x", "javascript:alert(1)", "wiki", "/a b", "ftp://example.org/x", "https://"):
    code, out = call(owner, "add_link", {"site": s, "title": "Bad", "url": bad})
    check(f"a link to {bad!r} is refused", code != 200, (code, out))
code, out = call(owner, "add_link", {"site": s, "title": " ", "url": "/x"})
check("a link needs a title", code != 200, (code, out))
links = ok(owner, "site_links", {"site": s})
check("links come in the order they were added", [x["title"] for x in links] == ["Pipeline", "Handbook", "Wiki"], links)
ok(bob, "link_up", {"id": l3})
ok(bob, "link_down", {"id": l1})
check("links move", [x["title"] for x in ok(owner, "site_links", {"site": s})] == ["Wiki", "Pipeline", "Handbook"])
check("the first cannot go up", ok(owner, "link_up", {"id": l3}) == {"moved": False})
page = ok(bob, "view_site", {"site": s})["page"]
check("the start page lists the links, in order, with their words", page.index("[Wiki](/wiki)") < page.index("[Pipeline](/crm) — Open deals") < page.index("[Handbook](https://example.org/handbook)"), page)
ok(owner, "delete_link", {"id": l2})
check("a link is deleted and the others close up", [x["number"] for x in ok(owner, "site_links", {"site": s})] == ["1.", "2."])
code, out = call(cy, "delete_link", {"id": l1})
check("a stranger cannot delete a link", code != 200, (code, out))

# --- who may change what
code, out = call(bob, "edit_site", {"site": s, "name": "Renamed"})
check("an editor cannot rename the site", code != 200, (code, out))
code, out = call(bob, "edit_site", {"site": s, "open": True})
check("nor open it to everyone", code != 200, (code, out))
ok(bob, "edit_site", {"site": s, "home": "# New words"})
check("an editor writes the start page", "# New words" in ok(owner, "view_site", {"site": s})["page"])
code, out = call(bob, "delete_site", {"site": s})
check("only an owner deletes a site", code != 200, (code, out))
code, out = call(bob, "add_site_members", {"site": s, "users": [cy_id]})
check("only an owner adds people", code != 200, (code, out))
ok(owner, "add_site_members", {"site": s, "users": [cy_id], "role": "visitor"})
check("a visitor sees the site", any(x["id"] == s and x["role"] == "visitor" for x in ok(cy, "my_sites")))
check("and its page", "New words" in ok(cy, "view_site", {"site": s})["page"])
code, out = call(cy, "add_link", {"site": s, "title": "x", "url": "/x"})
check("a visitor cannot add links", code != 200, (code, out))
code, out = call(cy, "edit_site", {"site": s, "home": "x"})
check("nor write the page", code != 200, (code, out))
code, out = call(owner, "remove_site_member", {"site": s, "id": owner_id})
check("the last owner cannot leave", code != 200, (code, out))
ok(owner, "set_site_role", {"site": s, "id": cy_id, "role": "editor"})
ok(cy, "add_link", {"site": s, "title": "By cy", "url": "/cy"})
ok(cy, "remove_site_member", {"site": s, "id": cy_id})
check("anyone may leave", all(x["id"] != s for x in ok(cy, "my_sites")))

# --- open sites and the news
ok(owner, "edit_site", {"site": s, "open": True})
check("an open site is visited by everyone", any(x["id"] == s and x["role"] == "visitor" and not x["member"] for x in ok(cy, "my_sites")))
check("they can open it", call(cy, "view_site", {"site": s})[0] == 200)
code, out = call(cy, "edit_site", {"site": s, "home": "x"})
check("but not change it", code != 200, (code, out))
post = ok(owner, "create_post", {"title": "Record quarter " + RUN, "summary": "Sales are up", "body": "Well done", "publish": True}, "atrium_news")["id"]
check("news is not on the page unless the site asks", "Latest news" not in ok(cy, "view_site", {"site": s})["page"])
ok(owner, "edit_site", {"site": s, "show_news": True})
news_page = ok(cy, "view_site", {"site": s})["page"]
check("with news on, the latest posts are listed with links", f"[Record quarter {RUN}](/news/p/{post}) — Sales are up" in news_page, news_page)
ok(owner, "delete_post", {"id": post}, "atrium_news")
ok(owner, "edit_site", {"site": s, "open": False})
check("closing the site hides it again", all(x["id"] != s for x in ok(cy, "my_sites")))

# --- deleting
ok(owner, "delete_site", {"site": s})
check("a deleted site is gone", call(owner, "view_site", {"site": s})[0] != 200 and all(x["id"] != s for x in ok(owner, "my_sites")))

finish()
