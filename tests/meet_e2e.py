"""Atrium meetings, end to end over HTTP: the administrator points the organization at a meeting server, people plan and
start meetings, invite colleagues, and join with a link made for them (signed, one room, moderator only for the host)."""
import base64, json
from e2e_lib import *

PLUGIN = "atrium_meet"


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


def claims(url):
    jwt = url.split("jwt=")[1].split("#")[0]
    return json.loads(base64.urlsafe_b64decode(jwt.split(".")[1] + "=="))


owner, bob, cy = login(USER), login("bob"), login("cy")
people = {p["name"]: p["id"] for p in owner.call("POST", "/api/plugins/atrium_chat/list_people", {})[1]}
bob_id, cy_id = people["bob"], people["cy"]

code, out = owner.call("PATCH", "/api/ui/bridges/jitsi", {"enabled": True})
check("an administrator switches the bridge on", code == 200, (code, out))
for k, v in [("bridge.jitsi.base_url", "https://meet.acme.test"), ("bridge.jitsi.app_id", "aether"), ("bridge.jitsi.signing_key", "s3cret-" + RUN)]:
    code, out = owner.call("PUT", f"/api/settings/{k}", {"value": v})
    check(f"setting {k}", code in (200, 204), (code, out))

# --- planning
code, out = call(bob, "create_meeting", {"title": "  "})
check("a meeting needs a title", code != 200, (code, out))
code, out = call(bob, "create_meeting", {"title": "Odd", "minutes": 2})
check("and a sensible length", code != 200, (code, out))
code, out = call(bob, "create_meeting", {"title": "Odd", "starts_at": "tomorrow"})
check("and a real start", code != 200, (code, out))
mid = ok(bob, "create_meeting", {"title": "Planning " + RUN, "starts_at": "2099-01-05T10:00", "minutes": 45, "users": [cy_id, bob_id, "users:nobody"], "about": "Quarter plan"})["id"]
g = ok(bob, "get_meeting", {"id": mid})
check("the host sees it, with the guests (not himself, not strangers)", g["guests"] == "cy" and g["role"] == "Host" and g["minutes"] == 45, g)
check("the guest sees it as invited", ok(cy, "get_meeting", {"id": mid})["role"] == "Invited")
code, out = call(owner, "get_meeting", {"id": mid})
check("someone not invited cannot open it", code != 200, (code, out))
code, out = call(owner, "join_meeting", {"id": mid})
check("nor join it", code != 200, (code, out))
check("it is on the list of the host and the guest only", any(m["id"] == mid for m in ok(bob, "my_meetings")) and any(m["id"] == mid for m in ok(cy, "my_meetings")) and not any(m["id"] == mid for m in ok(owner, "my_meetings")))

# --- the calendar
cal = lambda b, fn_, body=None: b.call("POST", f"/api/plugins/atrium_calendar/{fn_}", body or {})
code, evs = cal(bob, "events_in", {"start": "2099-01-01T00:00:00Z", "end": "2099-01-31T00:00:00Z"})
mine_ev = [e for e in evs if e["title"] == "Planning " + RUN] if code == 200 else []
check("a planned meeting is in the host's calendar", len(mine_ev) == 1 and mine_ev[0]["starts_at"].startswith("2099-01-05T10:00") and mine_ev[0]["location"] == "/meet/m/" + mid, (code, evs))
code, evs_cy = cal(cy, "events_in", {"start": "2099-01-01T00:00:00Z", "end": "2099-01-31T00:00:00Z"})
check("and in the guest's, who is invited", code == 200 and any(e["title"] == "Planning " + RUN for e in evs_cy), (code, evs_cy))

# --- joining
host_url = ok(bob, "join_meeting", {"id": mid})["open_url"]
c = claims(host_url)
check("the link is on the meeting server and signed", host_url.startswith("https://meet.acme.test/aether-" + mid + "?jwt="), host_url)
check("the host is the moderator, named", c["moderator"] is True and c["context"]["user"]["name"] == "bob" and c["room"] == "aether-" + mid, c)
guest_url = ok(cy, "join_meeting", {"id": mid})["open_url"]
c2 = claims(guest_url)
check("a guest is not", c2["moderator"] is False and c2["context"]["user"]["name"] == "cy" and c2["room"] == c["room"], c2)
check("the link is good for hours, not forever", 0 < c["exp"] - c["nbf"] <= 4 * 3600 + 20, c)

# --- now
now = ok(owner, "start_now", {"title": "Quick " + RUN, "users": [bob_id]})
check("a meeting started now gives the host the link at once", now["open_url"].startswith("https://meet.acme.test/aether-" + now["id"]) and claims(now["open_url"])["moderator"] is True, now)
check("and the guest sees it listed as now", any(m["id"] == now["id"] and m["when"] == "Now" for m in ok(bob, "my_meetings")))

# --- cancelling
code, out = call(cy, "cancel_meeting", {"id": mid})
check("a guest cannot cancel", code != 200, (code, out))
ok(bob, "cancel_meeting", {"id": mid})
code, evs = cal(bob, "events_in", {"start": "2099-01-01T00:00:00Z", "end": "2099-01-31T00:00:00Z"})
check("cancelling takes the event out of the calendar", code == 200 and not any(e["title"] == "Planning " + RUN for e in evs), (code, evs))
code, out = call(cy, "get_meeting", {"id": mid})
check("a cancelled meeting is gone for everyone", code != 200, (code, out))
ok(owner, "cancel_meeting", {"id": now["id"]})
finish()
