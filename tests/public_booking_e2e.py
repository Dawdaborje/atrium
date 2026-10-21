"""Atrium booking for people outside the organization, end to end over HTTP: a member makes a service public, a visitor with
no account sees the free times, books one, finds the booking by its token and cancels it. See README.md."""
import datetime
from e2e_lib import *

PLUGIN = "atrium_booking"
ORG = "atr_org"


def login(user):
    b = Browser()
    b.call("GET", "/api/auth/methods")
    code, body = b.call("POST", "/api/auth/login", {"username": user, "password": PASSWORD})
    assert code == 200, (user, code, body)
    return b


def call(b, fn, body=None, plugin=PLUGIN):
    return b.call("POST", f"/api/plugins/{plugin}/{fn}", body or {}, headers={"X-Org-Slug": ORG})


def ok(b, fn, body=None, plugin=PLUGIN):
    code, out = call(b, fn, body, plugin)
    assert code == 200, (fn, code, out)
    return out


def visitor():
    """A browser with no account: it first opens a public page, which gives it the visitor cookie."""
    b = Browser()
    b.call("GET", "/api/ui/pages/book/nothing/here", csrf=False, headers={"X-Org-Slug": ORG})
    return b


owner, bob = login(USER), login("bob")
today = datetime.date.today()
day = lambda n: (today + datetime.timedelta(days=n)).isoformat()

# (a run that stopped halfway leaves guest meetings behind)
for b in ok(owner, "my_bookings"):
    if "Gus Guest" in b["with"]:
        ok(owner, "cancel_booking", {"id": b["id"]})
svc = ok(owner, "create_service", {"title": "Open hours " + RUN, "minutes": "30", "weekdays": ["1", "2", "3", "4", "5", "6", "7"], "from_time": "09:00", "to_time": "12:00", "tz": "Europe/Oslo", "buffer": 0, "notice_hours": 0, "days_ahead": 10})["id"]
form = ok(owner, "get_service", {"id": svc})
check("a service is private at first and keeps its zone", form["public"] is False and form["public_path"] == "" and form["tz"] == "Europe/Oslo", form)
code, out = call(owner, "update_service", {"id": svc, "tz": "Mars/Base"})
check("a zone must be a real one", code != 200, (code, out))
ok(owner, "update_service", {"id": svc, "public": True})
form = ok(owner, "get_service", {"id": svc})
parts = form["public_path"].strip("/").split("/")
check("making it public gives an address with a token", form["public"] is True and len(parts) == 3 and parts[0] == "book" and parts[2] == svc, form["public_path"])
link = parts[1]
ref = {"link": link, "service": svc}

v = visitor()
code, out = call(v, "public_service", {"link": "wrong", "service": svc})
check("a wrong token shows nothing", code != 200, (code, out))
code, out = call(v, "public_slots", {"link": link, "service": "nosuch"})
check("nor does a wrong service", code != 200, (code, out))
info = ok(v, "public_service", ref)
check("a visitor sees what is offered, who by and the zone", info["title"] == "Open hours " + RUN and info["host"] == "owner" and "Europe/Oslo" in info["subtitle"], info)
slots = ok(v, "public_slots", ref)
check("and its free times, in the host's zone", len(slots) > 10 and all(len(s["id"]) == 13 and ":" not in s["id"] for s in slots), slots[:2])
check("none outside the hours (09:00 to 12:00 in Oslo is 07:00 to 10:00 in summer, 08:00 to 11:00 in winter)", all(s["name"].split(", ")[1][:5] in ("09:00", "09:30", "10:00", "10:30", "11:00", "11:30") for s in slots), slots[:3])

# --- booking
pick = slots[3]
code, out = call(v, "public_book", {**ref, "slot": pick["id"], "name": "Gus Guest", "email": "not an address"})
check("an email address is needed", code != 200, (code, out))
code, out = call(v, "public_book", {**ref, "slot": pick["id"], "name": " ", "email": "gus@example.org"})
check("and a name", code != 200, (code, out))
code, out = call(v, "public_book", {**ref, "slot": "20000101T0900", "name": "Gus", "email": "gus@example.org"})
check("a time that is not offered is refused", code != 200, (code, out))
made = ok(v, "public_book", {**ref, "slot": pick["id"], "name": "Gus Guest", "email": "Gus@Example.org", "note": "Hello " + RUN})
token = made["token"]
check("booking gives a token", len(token) > 10, made)
after = ok(v, "public_slots", ref)
check("the time is taken for the next visitor", pick["id"] not in [s["id"] for s in after] and len(after) == len(slots) - 1)
code, out = call(visitor(), "public_book", {**ref, "slot": pick["id"], "name": "Late", "email": "late@example.org"})
check("nobody else can book it", code != 200, (code, out))
mine = ok(v, "public_booking", {"token": token})
check("the visitor finds the booking by its token", mine["status"] == "confirmed" and mine["guest"] == "Gus Guest" and mine["with"] == "owner" and "Europe/Oslo" in mine["when"], mine)
code, out = call(visitor(), "public_booking", {"token": "nosuchtoken"})
check("a wrong token finds nothing", code != 200, (code, out))
for n in range(2, 4):
    ok(v, "public_book", {**ref, "slot": slots[3 + n]["id"], "name": "Gus Guest", "email": "gus@example.org"})
code, out = call(v, "public_book", {**ref, "slot": slots[9]["id"], "name": "Gus Guest", "email": "gus@example.org"})
check("a guest holds only a few meetings at once", code != 200, (code, out))

# --- the host's side
rows = ok(owner, "my_bookings")
guest_rows = [r for r in rows if "Gus Guest" in r["with"]]
check("the host sees the guest and the note's meeting among their bookings", len(guest_rows) == 3 and guest_rows[0]["you_are"] == "host", rows)
events = ok(owner, "events_in", {"start": day(0) + "T00:00:00Z", "end": day(12) + "T00:00:00Z"}, "atrium_calendar")
mine_ev = [e for e in events if e["title"].startswith("Open hours " + RUN)]
check("their calendar has the meetings, with the guest in the notes", len(mine_ev) == 3, events)
detail = ok(owner, "get_event", {"id": mine_ev[0]["id"]}, "atrium_calendar")
check("the notes name the guest and the address", "Gus Guest <gus@example.org>" in detail["description"] and "Hello " + RUN in "".join(ok(owner, "get_event", {"id": e["id"]}, "atrium_calendar")["description"] for e in mine_ev), detail)
code, out = call(bob, "free_slots", {"id": svc})
check("a colleague sees the same times minus the booked ones", code == 200 and len(out) == len(slots) - 3, (code, len(out) if isinstance(out, list) else out))

# --- cancelling
ok(v, "public_cancel", {"token": token})
check("the visitor cancels with the token", ok(v, "public_booking", {"token": token})["status"] == "cancelled")
code, out = call(v, "public_cancel", {"token": token})
check("not twice", code != 200, (code, out))
check("the time is free again", pick["id"] in [s["id"] for s in ok(v, "public_slots", ref)])
check("and the event left the host's calendar", len([e for e in ok(owner, "events_in", {"start": day(0) + "T00:00:00Z", "end": day(12) + "T00:00:00Z"}, "atrium_calendar") if e["title"].startswith("Open hours " + RUN)]) == 2)
other = [r for r in ok(owner, "my_bookings") if "Gus Guest" in r["with"]][0]
ok(owner, "cancel_booking", {"id": other["id"]})
check("the host cancels a guest's booking too", len([r for r in ok(owner, "my_bookings") if "Gus Guest" in r["with"]]) == 1)

# --- switching off
ok(owner, "update_service", {"id": svc, "public": False})
code, out = call(v, "public_slots", ref)
check("a service taken off the public stops answering visitors", code != 200, (code, out))
ok(owner, "update_service", {"id": svc, "public": True})
ok(owner, "revoke_link", {}, "atrium_calendar")
code, out = call(v, "public_slots", ref)
check("and so does revoking the link", code != 200, (code, out))
code, out = call(v, "link_busy", {"token": link, "start": day(0) + "T00:00:00Z", "end": day(1) + "T00:00:00Z"}, "atrium_calendar")
check("the calendar's own link answers no one after that", code != 200, (code, out))
code, out = call(v, "busy_of", {"user": "owner", "start": day(0) + "T00:00:00Z", "end": day(1) + "T00:00:00Z"}, "atrium_calendar")
check("a visitor cannot read anyone's busy times without a link", code != 200, (code, out))
code, out = call(v, "my_services")
check("nor reach the member functions", code != 200, (code, out))

for b in ok(owner, "my_bookings"):
    if "Gus Guest" in b["with"]:
        ok(owner, "cancel_booking", {"id": b["id"]})
ok(owner, "delete_service", {"id": svc})
finish()
