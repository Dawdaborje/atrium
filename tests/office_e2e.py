"""Editing office files in the browser, end to end over HTTP. Plays the editor server (Collabora): takes the address the drive
gives, reads the file and sends it back through the kernel's WOPI endpoints with the access token. See README.md."""
import io, json, urllib.parse, urllib.request, urllib.error, zipfile
from e2e_lib import *

PLUGIN = "atrium_drive"
KEY = "office-secret-" + RUN + "-0123456789"


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
    req = urllib.request.Request(BASE + "/api/media?name=" + urllib.parse.quote(name), data=data, method="POST",
                                 headers={"Content-Type": "application/octet-stream", "Origin": BASE, "X-CSRF-Token": b.csrf()})
    with b.opener.open(req) as r:
        return json.loads(r.read())


def docx(text):
    out = io.BytesIO()
    with zipfile.ZipFile(out, "w") as z:
        z.writestr(zipfile.ZipInfo("[Content_Types].xml", (2026, 1, 1, 0, 0, 0)), "<Types/>")
        z.writestr(zipfile.ZipInfo("word/document.xml", (2026, 1, 1, 0, 0, 0)), f"<w:document>{text}</w:document>")
    return out.getvalue()


def editor(method, path, token, body=None):
    """The editor's call: no cookies, no CSRF token, only the access token."""
    req = urllib.request.Request(BASE + path + ("&" if "?" in path else "?") + "access_token=" + urllib.parse.quote(token), data=body, method=method)
    try:
        with urllib.request.urlopen(req) as r:
            return r.status, r.read()
    except urllib.error.HTTPError as e:
        return e.code, e.read()


def parts(url):
    q = urllib.parse.parse_qs(urllib.parse.urlparse(url).query)
    src = urllib.parse.urlparse(q["WOPISrc"][0]).path
    return src, q["access_token"][0]


owner, bob, cy = login(USER), login("bob"), login("cy")
people = {p["name"]: p["id"] for p in owner.call("POST", "/api/plugins/atrium_chat/list_people", {})[1]}
bob_id, cy_id = people["bob"], people["cy"]

code, out = call(owner, "open_in_editor", {"file": "nothing"})
check("a file that does not exist cannot be opened", code != 200, (code, out))
code, out = owner.call("PATCH", "/api/ui/bridges/collabora", {"enabled": True})
check("an administrator switches the editor bridge on", code == 200, (code, out))
for k, v in [("base_url", "https://office.acme.test"), ("wopi_host_url", BASE), ("signing_key", KEY)]:
    code, out = owner.call("PUT", f"/api/settings/bridge.collabora.{k}", {"value": v})
    check(f"setting {k}", code in (200, 204), (code, out))

# --- a document and a PDF
media = upload(owner, f"Plan {RUN}.docx", docx("first draft"))
fid = ok(owner, "register_file", {"media": media})["id"]
pic = upload(owner, f"scan{RUN}.pdf", b"%PDF-1.4\n1 0 obj<<>>endobj\ntrailer<<>>\n%%EOF\n")
pid = ok(owner, "register_file", {"media": pic})["id"]
code, out = call(owner, "open_in_editor", {"file": pid})
check("a PDF is not edited online", code != 200, (code, out))

# --- the address
opened = ok(owner, "open_in_editor", {"file": fid})
src, token = parts(opened["open_url"])
check("the owner gets an editor address, with write rights", opened["open_url"].startswith("https://office.acme.test/browser/dist/cool.html?WOPISrc=") and opened["can_write"] is True, opened)
code, out = call(bob, "open_in_editor", {"file": fid})
check("someone with no access cannot ask for one", code != 200, (code, out))

# --- the editor reads
code, body = editor("GET", src, token)
info = json.loads(body) if code == 200 else {}
check("CheckFileInfo names the file and the person, and says they may write", code == 200 and info["BaseFileName"] == f"Plan {RUN}.docx" and info["UserFriendlyName"] == "owner" and info["UserCanWrite"] is True and info["Size"] == media["size"], (code, body[:200]))
code, body = editor("GET", src + "/contents", token)
check("GetFile sends the bytes", code == 200 and body == docx("first draft"), (code, body[:60]))

# --- the token is for this file only
other_src = src.rsplit("/", 1)[0] + "/" + __import__("base64").urlsafe_b64encode(pic["key"].encode()).decode().rstrip("=")
code, _ = editor("GET", other_src + "/contents", token)
check("the token does not open another file", code == 401, code)
code, _ = editor("GET", src + "/contents", token[:-3] + ("AAA" if not token.endswith("AAA") else "BBB"))
check("a changed token is refused", code == 401, code)
code, _ = editor("GET", src + "/contents", "garbage")
check("so is garbage", code == 401, code)
req = urllib.request.Request(BASE + src + "/contents")
try:
    urllib.request.urlopen(req)
    code = 200
except urllib.error.HTTPError as e:
    code = e.code
check("without a token: 401", code == 401, code)

# --- the editor saves: a new version, by the person
code, body = editor("POST", src + "/contents", token, docx("second draft"))
check("PutFile is accepted", code == 200 and "LastModifiedTime" in json.loads(body), (code, body[:200]))
vs = ok(owner, "file_versions", {"file": fid})
check("it became a new version of the file", len(vs) == 2, vs)
code, body = editor("GET", src, token)
check("the file is the new one when the editor asks again", code == 200, code)
opened2 = ok(owner, "open_in_editor", {"file": fid})
src2, token2 = parts(opened2["open_url"])
code, body = editor("GET", src2 + "/contents", token2)
check("and its bytes are the second draft", body == docx("second draft"), body[:60])
code, body = editor("POST", src2 + "/contents", token2, b"not an allowed kind of file \x00\x01")
check("something that is not an accepted kind of file is refused", code == 415, (code, body[:100]))
check("and made no version", len(ok(owner, "file_versions", {"file": fid})) == 2)

# --- someone who may only look
ok(owner, "share_item", {"kind": "file", "id": fid, "user": bob_id, "level": "view"})
v = ok(bob, "open_in_editor", {"file": fid})
check("a viewer gets the editor without saving", v["can_write"] is False)
vsrc, vtok = parts(v["open_url"])
code, body = editor("GET", vsrc, vtok)
check("CheckFileInfo says read-only", code == 200 and json.loads(body)["UserCanWrite"] is False and json.loads(body)["ReadOnly"] is True, (code, body[:200]))
code, body = editor("POST", vsrc + "/contents", vtok, docx("sneaky"))
check("and a save with that token is refused", code == 403, (code, body[:100]))

# --- a person whose rights were taken away after the token was made
ok(owner, "share_item", {"kind": "file", "id": fid, "user": cy_id, "level": "edit"})
c = ok(cy, "open_in_editor", {"file": fid})
csrc, ctok = parts(c["open_url"])
ok(owner, "unshare_item", {"kind": "file", "id": fid, "user": cy_id})
code, body = editor("POST", csrc + "/contents", ctok, docx("late edit"))
check("a save by someone no longer allowed to edit is refused by the drive", code in (403, 500), (code, body[:100]))
check("and made no version", len(ok(owner, "file_versions", {"file": fid})) == 2, ok(owner, "file_versions", {"file": fid}))

# --- trash
ok(owner, "trash_item", {"kind": "file", "id": fid})
code, out = call(owner, "open_in_editor", {"file": fid})
check("a file in the trash is not edited", code != 200, (code, out))
finish()
