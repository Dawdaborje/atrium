"""Atrium drive, end to end over HTTP: folders, files, sharing, versions, the trash, and above all that a guarded file is
served only to the people the drive allows. See README.md for the one-time setup. Exits 1 when a check fails."""
import json, urllib.request, urllib.error
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


def upload(b, name, data):
    """Upload bytes through the media store, as the browser does; returns the file description."""
    req = urllib.request.Request(BASE + "/api/media?name=" + urllib.parse.quote(name), data=data, method="POST",
                                 headers={"Content-Type": "text/plain", "Origin": BASE, "X-CSRF-Token": b.csrf()})
    with b.opener.open(req) as r:
        return json.loads(r.read())


def read(b, key):
    """The status of reading a stored file, and its bytes."""
    req = urllib.request.Request(BASE + "/api/media/" + key, headers={"Origin": BASE})
    try:
        with b.opener.open(req) as r:
            return r.status, r.read()
    except urllib.error.HTTPError as e:
        return e.code, e.read()


def put(b, name, text, folder=None):
    media = upload(b, name, text.encode())
    out = ok(b, "register_file", {"media": media, **({"folder": folder} if folder else {})})
    return media, out


def names(listing):
    return sorted([f["name"] for f in listing["folders"]] + [f["name"] for f in listing["files"]])


owner, bob, cy = login(USER), login("bob"), login("cy")
import urllib.parse
chat_people = owner.call("POST", "/api/plugins/atrium_chat/list_people", {})[1]
people = {p["name"]: p["id"] for p in chat_people}
bob_id, cy_id = people["bob"], people["cy"]

# --- a file's bytes, before and after the drive guards them
media = upload(owner, f"plan-{RUN}.txt", b"version one " + RUN.encode())
st, _ = read(bob, media["key"])
check("an uploaded file is readable by every member until it is registered (as before)", st == 200, st)
reg = ok(owner, "register_file", {"media": media})
f1 = reg["id"]
check("registering names the file after the upload", reg["name"] == f"plan-{RUN}.txt" and reg["version"] == 1, reg)
st, data = read(owner, media["key"])
check("the owner reads their file", st == 200 and data == b"version one " + RUN.encode(), (st, data))
st, _ = read(bob, media["key"])
check("another member can no longer read the bytes", st == 403, st)
st, _ = read(cy, media["key"])
check("...nor anyone else", st == 403, st)
anon = Browser()
st, _ = read(anon, media["key"])
check("someone not signed in reads nothing", st == 401, st)

# --- browsing
mine = ok(owner, "drive_browse")
check("the file is in the owner's files", f"plan-{RUN}.txt" in names(mine) and mine["can_write"], mine)
check("it is not in anyone else's", f"plan-{RUN}.txt" not in names(ok(bob, "drive_browse")))
code, out = call(bob, "file_info", {"file": f1})
check("a stranger cannot even look at it (it does not exist for them)", code != 200, (code, out))
info = ok(owner, "file_info", {"file": f1})
check("file info gives the level, version and size", info["level"] == "owner" and info["version"] == 1 and info["size"] == len(b"version one " + RUN.encode()), info)

# --- names
dup = put(owner, f"plan-{RUN}.txt", "again")[1]
check("a second file with the same name gets a number", dup["name"] == f"plan-{RUN} (2).txt", dup)
for bad in ["a/b", "x" * 201, "  ", ".."]:
    code, out = call(owner, "create_folder", {"name": bad})
    check(f"the folder name {bad[:12]!r} is cleaned or refused", (code == 200 and "/" not in out["name"]) or code != 200, (code, out))
code, out = call(owner, "register_file", {"media": {"key": "plugins/x/y.txt", "name": "y.txt"}})
check("a file that did not come from the media store is refused", code != 200, (code, out))

# --- sharing a file
code, out = call(bob, "share_item", {"kind": "file", "id": f1, "user": cy_id})
check("only the owner shares", code != 200, (code, out))
ok(owner, "share_item", {"kind": "file", "id": f1, "user": bob_id, "level": "view"})
st, data = read(bob, media["key"])
check("a person it is shared with can read the bytes", st == 200 and data.startswith(b"version one"), st)
check("...and sees it under Shared with me", f"plan-{RUN}.txt" in names(ok(bob, "drive_browse", {"view": "shared"})))
code, out = call(bob, "rename_item", {"kind": "file", "id": f1, "name": "mine now.txt"})
check("view access does not allow changes", code != 200, (code, out))
code, out = call(bob, "trash_item", {"kind": "file", "id": f1})
check("...nor trashing", code != 200, (code, out))
st, _ = read(cy, media["key"])
check("someone it is not shared with still cannot read it", st == 403, st)
ok(owner, "share_item", {"kind": "file", "id": f1, "user": bob_id, "level": "edit"})
check("sharing again changes the level", ok(bob, "file_info", {"file": f1})["level"] == "edit")
sh = ok(owner, "list_shares", {"kind": "file", "id": f1})
check("the owner lists who it is shared with", len(sh) == 1 and sh[0]["level"] == "edit" and sh[0]["name"] == "bob", sh)
code, out = call(bob, "list_shares", {"kind": "file", "id": f1})
check("others do not see the list", code != 200, (code, out))
ok(bob, "rename_item", {"kind": "file", "id": f1, "name": f"plan-{RUN}-v2.txt"})
check("edit access allows renaming", ok(owner, "file_info", {"file": f1})["name"] == f"plan-{RUN}-v2.txt")
ok(owner, "unshare_item", {"kind": "file", "id": f1, "user": bob_id})
st, _ = read(bob, media["key"])
check("unsharing takes the access away at once", st == 403, st)
ok(owner, "share_item", {"kind": "file", "id": f1, "user": "org", "level": "view"})
st, _ = read(cy, media["key"])
check("a share with the whole organization reaches everyone", st == 200, st)
ok(owner, "unshare_item", {"kind": "file", "id": f1, "user": "org"})
st, _ = read(cy, media["key"])
check("...until it is removed", st == 403, st)
code, out = call(owner, "share_item", {"kind": "file", "id": f1, "user": "nobody"})
check("sharing with a stranger is refused", code != 200, (code, out))
code, out = call(owner, "share_item", {"kind": "file", "id": f1, "user": bob_id, "level": "owner"})
check("ownership cannot be handed out by a share", code != 200, (code, out))

# --- folders: a share covers what is inside
docs = ok(owner, "create_folder", {"name": "Docs " + RUN})["id"]
sub = ok(owner, "create_folder", {"name": "Deep", "parent": docs})["id"]
code, out = call(owner, "create_folder", {"name": "Deep", "parent": docs})
check("a folder name is unique where it is", code != 200, (code, out))
m_in, in_file = put(owner, "inside.txt", "inside text", folder=sub)
check("a file goes into a nested folder", ok(owner, "drive_browse", {"folder": sub})["files"][0]["name"] == "inside.txt")
st, _ = read(cy, m_in["key"])
check("(before sharing) the folder's file is private", st == 403)
ok(owner, "share_item", {"kind": "folder", "id": docs, "user": cy_id, "level": "view"})
st, data = read(cy, m_in["key"])
check("a share on a folder covers a file two levels down", st == 200 and data == b"inside text", st)
deep = ok(cy, "drive_browse", {"folder": sub})
check("...which the person can browse, starting their path at the shared folder", [p["id"] for p in deep["path"]][0] == docs and deep["can_write"] is False, deep["path"])
code, out = call(cy, "create_folder", {"name": "nope", "parent": docs})
check("a view share lets nobody add to the folder", code != 200, (code, out))
code, out = call(cy, "register_file", {"folder": docs, "media": upload(cy, "x.txt", b"x")})
check("...or put a file in it", code != 200, (code, out))
ok(owner, "share_item", {"kind": "folder", "id": docs, "user": cy_id, "level": "edit"})
added = ok(cy, "register_file", {"folder": docs, "media": upload(cy, "by-cy.txt", b"from cy")})
check("with edit the person can add files", added["name"] == "by-cy.txt")
cy_file = ok(owner, "drive_browse", {"folder": docs})["files"]
check("the owner of the folder sees what others added", any(f["name"] == "by-cy.txt" and f["owner_name"] == "cy" for f in cy_file), cy_file)
cykey = next(f for f in cy_file if f["name"] == "by-cy.txt")["media"]["key"]
st, data = read(owner, cykey)
check("...and can read it, because it is in their folder", st == 200 and data == b"from cy", st)

cy_item = next(f for f in cy_file if f["name"] == "by-cy.txt")
check("the folder's owner manages what is in it (owner level)", cy_item["level"] == "owner", cy_item)
ok(owner, "trash_item", {"kind": "file", "id": cy_item["id"]})
ok(owner, "restore_item", {"kind": "file", "id": cy_item["id"]})

# --- moving
other = ok(owner, "create_folder", {"name": "Other " + RUN})["id"]
moved = ok(owner, "move_item", {"kind": "file", "id": f1, "to": other})
check("a file moves into another folder", ok(owner, "drive_browse", {"folder": other})["files"][0]["name"] == moved["name"])
code, out = call(owner, "move_item", {"kind": "folder", "id": docs, "to": sub})
check("a folder cannot move into itself or its own subfolder", code != 200, (code, out))
code, out = call(bob, "move_item", {"kind": "file", "id": f1, "to": other})
check("a stranger cannot move it", code != 200, (code, out))
bob_folder = ok(bob, "create_folder", {"name": "bobs " + RUN})["id"]
code, out = call(owner, "move_item", {"kind": "file", "id": f1, "to": bob_folder})
check("nor can anything be moved into someone else's folder", code != 200, (code, out))
ok(owner, "move_item", {"kind": "file", "id": f1})
check("moving with no target puts it back at the top", moved["name"] in names(ok(owner, "drive_browse")))
tree = ok(owner, "folder_tree")
check("the folder picker lists folders with full paths", any(t["name"] == f"Docs {RUN} / Deep" for t in tree), tree)
check("...only ones the caller may write to", all(t["id"] != bob_folder for t in tree))

# --- versions
media2 = upload(owner, "new.txt", b"version two")
v2 = ok(owner, "new_version", {"file": f1, "media": media2})
check("a new version raises the version number", v2["version"] == 2, v2)
st, data = read(owner, media2["key"])
check("the new bytes are guarded and readable by the owner", st == 200 and data == b"version two")
st, _ = read(bob, media2["key"])
check("...and not by strangers", st == 403)
vs = ok(owner, "file_versions", {"file": f1})
check("the versions list has both, newest first, the newest current", [v["version"] for v in vs] == [2, 1] and vs[0]["current"] and not vs[1]["current"], vs)
st, data = read(owner, vs[1]["media"]["key"])
check("the old version is still readable by the owner", st == 200 and data.startswith(b"version one"), st)
code, out = call(bob, "new_version", {"file": f1, "media": upload(bob, "z.txt", b"z")})
check("only someone with edit access adds a version", code != 200, (code, out))
back = ok(owner, "restore_version", {"file": f1, "version": 1})
check("restoring makes an old version current as a new version", back["version"] == 3 and ok(owner, "file_info", {"file": f1})["version"] == 3, back)
check("nothing is lost: three versions now", len(ok(owner, "file_versions", {"file": f1})) == 3)
ok(owner, "share_item", {"kind": "file", "id": f1, "user": bob_id, "level": "view"})
st, _ = read(bob, vs[1]["media"]["key"])
check("someone it is shared with reads every version", st == 200, st)

# --- the trash
ok(owner, "trash_item", {"kind": "file", "id": f1})
check("a trashed file leaves the folder listing", f"plan-{RUN}-v2.txt" not in names(ok(owner, "drive_browse")) and moved["name"] not in names(ok(owner, "drive_browse")))
check("...and shows in the trash", any(f["id"] == f1 for f in ok(owner, "drive_browse", {"view": "trash"})["files"]))
st, _ = read(owner, media["key"])
check("the owner can still read a trashed file", st == 200, st)
st, _ = read(bob, media["key"])
check("a person it was shared with cannot read it from the trash", st == 403, st)
check("it is gone from their Shared with me", all(f["id"] != f1 for f in ok(bob, "drive_browse", {"view": "shared"})["files"]))
ok(owner, "restore_item", {"kind": "file", "id": f1})
st, _ = read(bob, media["key"])
check("restoring brings it back, with its shares", st == 200 and any(f["id"] == f1 for f in ok(owner, "drive_browse")["files"]), st)
dead = ok(owner, "create_folder", {"name": "ToDelete " + RUN})["id"]
dm, dfile = put(owner, "gone.txt", "bye", folder=dead)
ok(owner, "trash_item", {"kind": "folder", "id": dead})
check("a trashed folder takes its contents out of view", all(f["id"] != dfile["id"] for f in ok(owner, "search_files", {"q": "gone.txt"})["files"]))
ok(owner, "restore_item", {"kind": "folder", "id": dead})
check("restoring the folder brings them back", any(f["id"] == dfile["id"] for f in ok(owner, "drive_browse", {"folder": dead})["files"]))
ok(owner, "trash_item", {"kind": "folder", "id": dead})
purged = ok(owner, "purge_item", {"kind": "folder", "id": dead})
check("deleting a folder for good deletes what is in it", purged["removed"] == 2, purged)
st, _ = read(owner, dm["key"])
check("...and its bytes are no longer served", st == 403, st)
ok(owner, "trash_item", {"kind": "file", "id": dup["id"]})
check("emptying the trash removes the rest", ok(owner, "empty_trash")["removed"] >= 1 and not ok(owner, "drive_browse", {"view": "trash"})["files"])
code, out = call(bob, "purge_item", {"kind": "file", "id": f1})
check("only the owner deletes for good", code != 200, (code, out))

# --- search, recent, usage
found = ok(owner, "search_files", {"q": "inside"})
check("search finds files by name", any(f["name"] == "inside.txt" for f in found["files"]), found)
check("...and folders", any(f["name"].startswith("Docs") for f in ok(owner, "search_files", {"q": "Docs " + RUN})["folders"]))
check("a one-letter search finds nothing", ok(owner, "search_files", {"q": "i"})["files"] == [])
check("search does not show what the person cannot see", all(f["name"] != "inside.txt" for f in ok(bob, "search_files", {"q": "inside"})["files"]))
check("recent lists the latest changes first", ok(owner, "drive_browse", {"view": "recent"})["files"][0]["modified_at"] is not None)
use = ok(owner, "storage_used")
check("storage counts every version", use["bytes"] > 0 and use["versions"] >= 4, use)

finish()
