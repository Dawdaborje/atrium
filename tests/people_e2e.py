"""Atrium people, end to end over HTTP: the directory, profiles, who may change what, reporting lines and the org chart,
and the fields the HR link keeps. See README.md."""
from e2e_lib import *

PLUGIN = "atrium_people"


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
bk, ck = bob_id.split(":")[-1], cy_id.split(":")[-1]
me_row = ok(owner, "my_profile")
ok_id = me_row["id"]
T = "T" + RUN

# --- the directory lists everyone, profile or not
names = [r["name"] for r in ok(bob, "directory")]
check("everyone who can sign in is listed", {"bob", "cy", "owner"} <= set(names), names)
check("sorted by name", names == sorted(names, key=str.lower))

# --- a person fills in their own profile
ok(bob, "save_profile", {"title": "Designer " + T, "department": "Product " + T, "phone": "123", "location": "Oslo", "skills": "figma, css", "bio": "Hello"})
p = ok(cy, "person", {"id": bk})
check("anyone can read a profile", p["title"] == "Designer " + T and p["department"] == "Product " + T and p["location"] == "Oslo", p)
check("and sees it is not theirs to edit", p["can_edit"] is False)
check("the owner of the profile can edit", ok(bob, "my_profile")["can_edit"] is True)
code, out = call(cy, "save_profile", {"id": bk, "title": "Boss"})
check("nobody changes someone else's profile", code != 200, (code, out))
ok(owner, "save_profile", {"id": bk, "mobile": "555"})
check("an administrator can", ok(cy, "person", {"id": bk})["mobile"] == "555")
code, out = call(bob, "save_profile", {"title": "x" * 200})
check("a too long field is refused", code != 200, (code, out))
code, out = call(bob, "save_profile", {"email": "not-an-address"})
check("an email must look like one", code != 200, (code, out))
code, out = call(bob, "save_profile", {"id": "nosuchperson", "title": "x"})
check("a made-up person is refused", code != 200, (code, out))

# --- search
found = [r["name"] for r in ok(cy, "directory", {"q": "figma"})]
check("search looks at skills", "bob" in found and "cy" not in found, found)
check("and at the department", "bob" in [r["name"] for r in ok(cy, "directory", {"q": "product " + T.lower()})])
check("a department filter narrows the list", [r["name"] for r in ok(cy, "directory", {"department": "Product " + T})] == ["bob"])
check("the departments are offered with counts", any(d["name"] == "Product " + T and d["count"] == 1 for d in ok(cy, "department_options")))
check("nothing matches nonsense", ok(cy, "directory", {"q": "zzzqqq" + RUN}) == [])

# --- reporting lines
ok(cy, "save_profile", {"title": "Engineer " + T, "manager": bob_id})
code, out = call(bob, "save_profile", {"manager": bob_id})
check("nobody reports to themselves", code != 200, (code, out))
code, out = call(bob, "save_profile", {"manager": cy_id})
check("a loop is refused", code != 200 and "loop" in str(out), (code, out))
code, out = call(cy, "save_profile", {"manager": "users:nobodyhere"})
check("a manager must be a member", code != 200, (code, out))
check("a person's manager is named", ok(bob, "person", {"id": ck})["manager_name"] == "bob")
check("reports are listed under the manager", [r["name"] for r in ok(owner, "person_reports", {"id": bk})] == ["cy"])
chain = ok(owner, "person_chain", {"id": ck})
check("the chain goes up through managers", [r["name"] for r in chain][:1] == ["bob"], chain)
chart = ok(owner, "org_chart", {"id": bk})
check("the chart from someone shows them and everyone below", [(r["name"], r["depth"]) for r in chart] == [("bob", 0), ("cy", 1)], chart)
full = ok(owner, "org_chart")
pos = {r["name"]: i for i, r in enumerate(full)}
check("the whole chart lists everyone once, managers before their reports", len(full) == len({r["id"] for r in full}) and pos["bob"] < pos["cy"], list(pos))
check("a person's row says how many report to them", next(r for r in full if r["name"] == "bob")["reports"] == 1)
ok(cy, "save_profile", {"manager": ""})
check("clearing the manager takes the line away", ok(owner, "person_reports", {"id": bk}) == [])
ok(cy, "save_profile", {"manager": bob_id})

# --- the HR link's fields
code, out = call(bob, "apply_hr", {"id": ck, "title": "Hacked"})
check("only the system or an administrator applies HR's data", code != 200, (code, out))
ok(owner, "apply_hr", {"id": ck, "title": "Staff Engineer " + T, "department": "Platform " + T, "email": "cy@example.test"})
c = ok(bob, "person", {"id": ck})
check("HR's fields land on the profile", c["title"] == "Staff Engineer " + T and c["department"] == "Platform " + T and c["email"] == "cy@example.test", c)
check("and are marked as HR's", sorted(c["hr_managed"]) == ["department", "email", "title"], c["hr_managed"])
code, out = call(cy, "save_profile", {"title": "CEO"})
check("the person cannot change what HR keeps", code != 200 and "HR" in str(out), (code, out))
ok(cy, "save_profile", {**{k: c[k] for k in ("title", "department", "email")}, "phone": "999"})
check("but a whole form sent back unchanged is accepted, and other fields change", ok(bob, "person", {"id": ck})["phone"] == "999")
ok(owner, "apply_hr", {"id": ck, "manager": cy_id})
check("a reporting line that would loop is left alone by HR's data", ok(bob, "person", {"id": ck})["manager"] == bob_id)
ok(owner, "release_hr", {"id": ck})
ok(cy, "save_profile", {"title": "Mine " + T})
check("released fields belong to the person again", ok(bob, "person", {"id": ck})["title"] == "Mine " + T)

check("pickers list people and can be searched", [p["name"] for p in ok(bob, "pick_people", {"q": "cy"})] == ["cy"])
finish()
