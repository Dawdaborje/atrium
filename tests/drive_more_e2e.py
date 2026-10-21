"""Atrium drive, second script: guarded uploads, the organization's limits (quota, links, search, comments), sharing by
link, comments, favourites and search through the text inside files. Run after drive_e2e.py. See README.md."""
import json, time, urllib.parse, urllib.request, urllib.error
from e2e_lib import *

PLUGIN = "atrium_drive"


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


def read(b, key):
    req = urllib.request.Request(BASE + "/api/media/" + key, headers={"Origin": BASE})
    try:
        with b.opener.open(req) as r:
            return r.status, r.read()
    except urllib.error.HTTPError as e:
        return e.code, e.read()


def put(b, name, text, folder=None, guard=True):
    st, media = upload(b, name, text.encode(), PLUGIN if guard else None)
    assert st == 200, (st, media)
    out = ok(b, "register_file", {"media": media, **({"folder": folder} if folder else {})})
    return media, out


owner, bob, cy = login(USER), login("bob"), login("cy")
people = {p["name"]: p["id"] for p in owner.call("POST", "/api/plugins/atrium_chat/list_people", {})[1]}
bob_id, cy_id = people["bob"], people["cy"]
set_ = lambda **kw: ok(owner, "save_drive_settings", kw)
set_(quota_mb=0, links_enabled=True, link_days=30, link_days_max=0, search_contents=True, comments_enabled=True)

# --- a guarded upload is closed from the first moment
st, media = upload(owner, f"closed-{RUN}.txt", b"private " + RUN.encode(), PLUGIN)
check("a file can be uploaded for a plugin that guards its files", st == 200, (st, media))
check("the uploader reads it before it is registered", read(owner, media["key"])[0] == 200)
check("nobody else does", read(bob, media["key"])[0] == 403)
ok(owner, "register_file", {"media": media})
check("after registering it is still the owner's alone", read(owner, media["key"])[0] == 200 and read(bob, media["key"])[0] == 403)
st, _ = upload(owner, "x.txt", b"hello", "atrium_chat")
check("a plugin that does not guard files cannot be named", st == 400, st)
st, _ = upload(owner, "x.txt", b"hello", "nosuchplugin")
check("nor one that is not installed", st == 400, st)
st, other = upload(owner, "y.txt", b"plain upload " + RUN.encode())
check("an upload without a guard is open to members, as before", st == 200 and read(bob, other["key"])[0] == 200)

# --- settings are for administrators
code, out = call(bob, "drive_settings")
check("an ordinary member cannot read the settings", code != 200, (code, out))
code, out = call(bob, "save_drive_settings", {"quota_mb": 5})
check("or change them", code != 200, (code, out))
check("an administrator reads them", ok(owner, "drive_settings")["links_enabled"] is True)
code, out = call(owner, "save_drive_settings", {"quota_mb": -1})
check("a negative quota is refused", code != 200, (code, out))
code, out = call(owner, "save_drive_settings", {"link_days": 40, "link_days_max": 30})
check("a default link life longer than the longest is refused", code != 200, (code, out))
nav = ok(owner, "drive_nav")
check("the places include Favourites", [n["id"] for n in nav] == ["mine", "favourites", "shared", "recent", "trash"], nav)
check("the administrator's listing offers the settings page", ok(owner, "drive_browse", {})["features"]["settings"] is True)
check("an ordinary member's does not", ok(bob, "drive_browse", {})["features"]["settings"] is False)

# --- quota
used0 = ok(owner, "storage_used")["bytes"]
MB = 1024 * 1024
set_(quota_mb=used0 // MB + 2)
check("the storage figure carries the limit", ok(owner, "storage_used")["quota_bytes"] == (used0 // MB + 2) * MB)
chunk = "x" * (600 * 1024)
accepted, refused = 0, None
for i in range(6):
    st, media = upload(owner, f"fill{i}-{RUN}.txt", (chunk + str(i)).encode(), PLUGIN)
    code, out = call(owner, "register_file", {"media": media})
    if code == 200:
        accepted += 1
    else:
        refused = (code, out)
        break
check("files are accepted until the quota is reached", accepted >= 1, accepted)
check("then one is refused, saying why", refused and "full" in json.dumps(refused[1]), refused)
check("what is stored is counted", ok(owner, "storage_used")["bytes"] >= used0 + accepted * 600 * 1024)
code, out = call(bob, "register_file", {"media": upload(bob, f"bobs-{RUN}.txt", b"small", PLUGIN)[1]})
check("the quota is for each person: someone else is not affected", code == 200, (code, out))
set_(quota_mb=0)
code, out = call(owner, "register_file", {"media": media})
check("with no quota the same file is accepted", code == 200, (code, out))

# --- links
folder = ok(owner, "create_folder", {"name": f"Public {RUN}"})["id"]
sub = ok(owner, "create_folder", {"name": "inner", "parent": folder})["id"]
m1, r1 = put(owner, f"in-folder-{RUN}.txt", "inside the folder " + RUN, folder)
m2, r2 = put(owner, f"deep-{RUN}.txt", "deep inside " + RUN, sub)
m3, r3 = put(owner, f"outside-{RUN}.txt", "outside " + RUN)
check("bob cannot read a folder's bytes before the link exists", read(bob, m1["key"])[0] == 403 and read(bob, m2["key"])[0] == 403)
link = ok(owner, "create_link", {"kind": "folder", "id": folder})
tok = link["path"].rsplit("/", 1)[1]
check("a link gives a path to open", link["path"].startswith("/drive/link/") and link["expires_at"], link)
check("bob cannot see the folder's files through the drive", call(bob, "file_info", {"file": r1["id"]})[0] != 200)
listing = ok(bob, "open_link", {"token": tok})
check("anyone in the organization can open the link", [f["name"] for f in listing["files"]] == [f"in-folder-{RUN}.txt"] and [f["name"] for f in listing["folders"]] == ["inner"] and listing["can_write"] is False, listing)
check("everything in it is view-only", all(f["level"] == "view" for f in listing["files"]))
check("the bytes of what the link shows are served", read(bob, m1["key"])[0] == 200 and read(cy, m1["key"])[0] == 200)
inner = ok(bob, "open_link", {"token": tok, "folder": sub})
check("a folder inside can be opened, with the path from the linked folder", [f["name"] for f in inner["files"]] == [f"deep-{RUN}.txt"] and [p["id"] for p in inner["path"]] == [folder, sub], inner)
check("...and its files are served too", read(bob, m2["key"])[0] == 200)
other_folder = ok(owner, "create_folder", {"name": f"Other {RUN}"})["id"]
code, out = call(bob, "open_link", {"token": tok, "folder": other_folder})
check("a folder outside the link cannot be reached through it", code != 200, (code, out))
check("a file outside the link stays closed", read(bob, m3["key"])[0] == 403)
code, out = call(bob, "open_link", {"token": "nosuchtoken"})
check("a made-up link does not open", code != 200, (code, out))
code, out = call(bob, "create_link", {"kind": "folder", "id": folder})
check("only the owner makes links", code != 200, (code, out))
check("the owner lists a link's details", ok(owner, "list_links", {"kind": "folder", "id": folder})[0]["text"].startswith("Works until"))
code, out = call(bob, "list_links", {"kind": "folder", "id": folder})
check("others cannot list them", code != 200, (code, out))
ok(owner, "trash_item", {"kind": "folder", "id": sub})
check("a trashed folder inside is no longer shown by the link", ok(bob, "open_link", {"token": tok})["folders"] == [])
ok(owner, "restore_item", {"kind": "folder", "id": sub})
ok(owner, "revoke_link", {"id": link["id"]})
code, out = call(bob, "open_link", {"token": tok})
check("a link taken back stops working", code != 200, (code, out))
check("and so do the bytes it showed", read(bob, m1["key"])[0] == 403)

fl = ok(owner, "create_link", {"kind": "file", "id": r3["id"]})
ftok = fl["path"].rsplit("/", 1)[1]
one = ok(cy, "open_link", {"token": ftok})
check("a link to one file shows that file", [f["name"] for f in one["files"]] == [f"outside-{RUN}.txt"] and one["folders"] == [], one)
check("and serves its bytes", read(cy, m3["key"])[0] == 200)
ok(owner, "trash_item", {"kind": "file", "id": r3["id"]})
code, out = call(cy, "open_link", {"token": ftok})
check("a file in the trash is not reached by its link", code != 200 and read(cy, m3["key"])[0] == 403, (code, out))
ok(owner, "restore_item", {"kind": "file", "id": r3["id"]})

# --- the organization's rules for links
set_(link_days_max=7, link_days=7)
code, out = call(owner, "create_link", {"kind": "file", "id": r3["id"], "days": 30})
check("a link cannot outlast the longest allowed", code != 200, (code, out))
code, out = call(owner, "create_link", {"kind": "file", "id": r3["id"], "days": 0})
check("nor be made without an end when there must be one", code != 200, (code, out))
short = ok(owner, "create_link", {"kind": "file", "id": r3["id"], "days": 3})
check("one within the limit is made", short["expires_at"] > "2000", short)
set_(links_enabled=False)
code, out = call(owner, "create_link", {"kind": "file", "id": r3["id"]})
check("with links switched off none can be made", code != 200, (code, out))
code, out = call(cy, "open_link", {"token": ftok})
check("and the ones made stop working", code != 200 and read(cy, m3["key"])[0] == 403, (code, out))
check("the listing says so, so the page hides the option", ok(owner, "drive_browse", {})["features"]["links"] is False)
set_(links_enabled=True, link_days_max=0, link_days=30)
check("switched back on, they work again", call(cy, "open_link", {"token": ftok})[0] == 200)

# --- comments
code, out = call(bob, "file_comments", {"file": r1["id"]})
check("a stranger cannot read a file's comments", code != 200, (code, out))
ok(owner, "share_item", {"kind": "file", "id": r1["id"], "user": bob_id, "level": "view"})
c1 = ok(bob, "add_comment", {"file": r1["id"], "body": "Looks good"})["id"]
rows = ok(owner, "file_comments", {"file": r1["id"]})
check("someone who can view a file can comment on it", [r["body"] for r in rows] == ["Looks good"] and rows[0]["author_name"] == "bob", rows)
check("the author and the owner may delete it", rows[0]["can_delete"] is True)
note = owner.call("GET", "/api/ui/notifications")[1]
check("the owner is told", any("commented on" in n.get("title", "") for n in note.get("notifications", note if isinstance(note, list) else [])))
code, out = call(bob, "add_comment", {"file": r1["id"], "body": "  "})
check("an empty comment is refused", code != 200, (code, out))
ok(owner, "add_comment", {"file": r1["id"], "body": "Thanks"})
code, out = call(bob, "delete_comment", {"id": [r for r in ok(bob, "file_comments", {"file": r1["id"]}) if r["body"] == "Thanks"][0]["id"]})
check("a comment can only be deleted by its author or the owner", code != 200, (code, out))
ok(owner, "delete_comment", {"id": c1})
check("the owner deletes anyone's", [r["body"] for r in ok(owner, "file_comments", {"file": r1["id"]})] == ["Thanks"])
set_(comments_enabled=False)
code, out = call(owner, "add_comment", {"file": r1["id"], "body": "more"})
check("with comments switched off nothing can be added", code != 200, (code, out))
check("the listing says so", ok(owner, "drive_browse", {})["features"]["comments"] is False)
set_(comments_enabled=True)

# --- favourites
ok(owner, "toggle_favourite", {"kind": "file", "id": r1["id"]})
fav = ok(owner, "drive_browse", {"view": "favourites"})
check("a starred file shows under Favourites", f"in-folder-{RUN}.txt" in [f["name"] for f in fav["files"]], fav)
check("and is marked in the folder's listing", [f["favourite"] for f in ok(owner, "drive_browse", {"folder": folder})["files"]] == [True])
check("stars are the person's own", ok(bob, "drive_browse", {"view": "favourites"})["files"] == [])
ok(owner, "toggle_favourite", {"kind": "folder", "id": folder, "on": True})
check("a folder can be starred too", f"Public {RUN}" in [f["name"] for f in ok(owner, "drive_browse", {"view": "favourites"})["folders"]])
ok(owner, "toggle_favourite", {"kind": "file", "id": r1["id"], "on": False})
check("a star is taken off", f"in-folder-{RUN}.txt" not in [f["name"] for f in ok(owner, "drive_browse", {"view": "favourites"})["files"]])
code, out = call(cy, "toggle_favourite", {"kind": "file", "id": r2["id"]})
check("a stranger cannot star what they cannot see", code != 200, (code, out))
ok(owner, "trash_item", {"kind": "folder", "id": folder})
check("a favourite in the trash is not listed", f"Public {RUN}" not in [f["name"] for f in ok(owner, "drive_browse", {"view": "favourites"})["folders"]])
ok(owner, "restore_item", {"kind": "folder", "id": folder})
ok(owner, "toggle_favourite", {"kind": "folder", "id": folder, "on": False})

# --- search through the text inside files
word = "zebrafish" + RUN
mt, rt = put(owner, f"notes-{RUN}.txt", f"minutes of the meeting about the {word} project")
found = []
for _ in range(30):
    found = ok(owner, "search_files", {"q": word})["files"]
    if found:
        break
    time.sleep(1)
check("a word that is only inside a file finds it", [f["name"] for f in found] == [f"notes-{RUN}.txt"] and found[0].get("found_in") == "text", found)
check("someone with no access does not find it", ok(cy, "search_files", {"q": word})["files"] == [])
check("a word in the name still finds it, as before", f"notes-{RUN}.txt" in [f["name"] for f in ok(owner, "search_files", {"q": f"notes-{RUN}"})["files"]])
set_(search_contents=False)
check("with content search off only names are searched", ok(owner, "search_files", {"q": word})["files"] == [])
set_(search_contents=True)
word2 = "narwhal" + RUN
mv = upload(owner, f"notes-{RUN}.txt", f"second version about {word2}".encode(), PLUGIN)[1]
ok(owner, "new_version", {"file": rt["id"], "media": mv})
found = []
for _ in range(30):
    found = ok(owner, "search_files", {"q": word2})["files"]
    if found:
        break
    time.sleep(1)
check("a new version is read again", len(found) == 1, found)

# --- purge removes the extras
ok(owner, "trash_item", {"kind": "file", "id": r1["id"]})
ok(owner, "purge_item", {"kind": "file", "id": r1["id"]})
code, out = call(owner, "file_comments", {"file": r1["id"]})
check("deleting a file for good takes its comments with it", code != 200, (code, out))

set_(quota_mb=0, links_enabled=True, link_days=30, link_days_max=0, search_contents=True, comments_enabled=True)
finish()
