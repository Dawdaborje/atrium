"""Atrium booking, end to end over HTTP: offering time, the free times (and that the calendar's busy time takes them
away), booking, the meeting in both calendars, taken times, cancelling from either side, and who may do what."""
import datetime
from e2e_lib import *

PLUGIN = "atrium_booking"


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
today = datetime.date.today()
day = lambda n: (today + datetime.timedelta(days=n)).isoformat()

# --- offering time
code, out = call(owner, "create_service", {"title": "x", "minutes": 30, "from_time": "10:00", "to_time": "10:10"})
check("hours shorter than one meeting are refused", code != 200, (code, out))
code, out = call(owner, "create_service", {"title": "x", "weekdays": []})
check("a service needs a day", code != 200, (code, out))
code, out = call(owner, "create_service", {"title": " ", "minutes": 30})
check("a service needs a title", code != 200, (code, out))
svc = ok(owner, "create_service", {"title": "Office hours " + RUN, "minutes": "30", "weekdays": ["1", "2", "3", "4", "5", "6", "7"], "from_time": "08:00", "to_time": "18:00", "buffer": 0, "notice_hours": 0, "days_ahead": 7, "location": "Room 1"})["id"]
mine = ok(owner, "my_services")
check("my services lists it", any(s["id"] == svc and s["minutes"] == 30 for s in mine), mine)
check("a colleague finds it", any(s["id"] == svc and s["length"] == "30 min" for s in ok(bob, "find_services", {"q": "office hours " + RUN.upper()[:0] + RUN})))
check("the host does not find their own", all(s["id"] != svc for s in ok(owner, "find_services")))
code, out = call(bob, "get_service", {"id": svc})
check("only the host can open it for editing", code != 200, (code, out))
code, out = call(bob, "update_service", {"id": svc, "title": "hacked"})
check("only the host can change it", code != 200, (code, out))
check("the editing form gets weekdays as text", sorted(ok(owner, "get_service", {"id": svc})["weekdays"]) == ["1", "2", "3", "4", "5", "6", "7"])

# --- free times
slots = ok(bob, "free_slots", {"id": svc})
check("free times are offered", len(slots) > 20, len(slots))
ids = [s["id"] for s in slots]
check("they start on the half hour and are in order", all(s["starts_at"][14:16] in ("00", "30") for s in slots) and ids == sorted(ids, key=lambda i: i.split("|")[1]))
check("none is in the past", all(s["starts_at"] > datetime.datetime.now(datetime.UTC).replace(tzinfo=None).strftime("%Y-%m-%dT%H:%M") for s in slots))
future = [s for s in slots if s["starts_at"].startswith(day(3))]
check("a day three days ahead has many slots", len(future) >= 10, len(future))
target = future[4]  # 10:00
check("the row says the time naturally", " – " in target["time"] and target["id"].startswith(svc + "|"), target)
check("and its id has no colon, which a page would cut at", ":" not in target["id"], target)

# --- the host's busy time takes slots away
busy_start = target["starts_at"]
busy_end = (datetime.datetime.strptime(busy_start, "%Y-%m-%dT%H:%M:%SZ") + datetime.timedelta(minutes=60)).strftime("%Y-%m-%dT%H:%M:%SZ")
ev = ok(owner, "create_event", {"title": "Private thing " + RUN, "starts_at": busy_start, "ends_at": busy_end}, "atrium_calendar")["id"]
after = ok(bob, "free_slots", {"id": svc})
taken = {s["starts_at"] for s in slots} - {s["starts_at"] for s in after}
check("an hour in the host's calendar removes two slots", len(taken) == 2 and busy_start in taken, taken)
check("and the title is never shown", "Private thing" not in json_dump if (json_dump := str(after)) else True)
code, out = call(bob, "book", {"id": target["id"]})
check("a busy time cannot be booked", code != 200, (code, out))

# --- booking
free_one = next(s for s in after if s["starts_at"].startswith(day(3)))
code, out = call(owner, "book", {"id": free_one["id"]})
check("you cannot book your own time", code != 200, (code, out))
code, out = call(bob, "book", {"id": "nonsense"})
check("a bad time is refused", code != 200, (code, out))
code, out = call(bob, "book", {"id": svc + "|" + day(3).replace("-", "") + "T0307"})
check("a time that is not a slot is refused", code != 200, (code, out))
booked = ok(bob, "book", {"id": free_one["id"], "note": "Questions about the plan"})
check("a colleague books a free time", booked.get("id") and booked.get("event"), booked)
code, out = call(cy, "book", {"id": free_one["id"]})
check("the same time cannot be booked twice", code != 200, (code, out))
check("the time is gone for everyone", all(s["starts_at"] != free_one["starts_at"] for s in ok(cy, "free_slots", {"id": svc})))
win = {"start": day(3) + "T00:00:00Z", "end": day(3) + "T23:59:59Z"}
host_events = ok(owner, "events_in", win, "atrium_calendar")
check("the meeting is in the host's calendar", any(e["id"] == booked["event"] and "Office hours" in e["title"] and e["location"] == "Room 1" for e in host_events), host_events)
guest_events = ok(bob, "events_in", win, "atrium_calendar")
check("and in the guest's, as an invitation", any(e["id"] == booked["event"] for e in guest_events), guest_events)
check("a stranger sees neither", all(e["id"] != booked["event"] for e in ok(cy, "events_in", win, "atrium_calendar")))
bm = ok(bob, "my_bookings")
check("the guest sees the booking", any(b["id"] == booked["id"] and b["you_are"] == "guest" and b["note"] == "Questions about the plan" for b in bm), bm)
hm = ok(owner, "my_bookings")
check("the host sees it too", any(b["id"] == booked["id"] and b["you_are"] == "host" for b in hm), hm)
check("a stranger sees nothing of it", all(b["id"] != booked["id"] for b in ok(cy, "my_bookings")))
code, out = call(owner, "delete_service", {"id": svc})
check("a service with bookings cannot be deleted", code != 200, (code, out))

# --- cancelling
code, out = call(cy, "cancel_booking", {"id": booked["id"]})
check("a stranger cannot cancel", code != 200, (code, out))
ok(bob, "cancel_booking", {"id": booked["id"]})
check("the guest cancels", all(b["id"] != booked["id"] for b in ok(bob, "my_bookings")))
check("it leaves the host's calendar", all(e["id"] != booked["event"] for e in ok(owner, "events_in", win, "atrium_calendar")))
check("and the time is free again", any(s["starts_at"] == free_one["starts_at"] for s in ok(cy, "free_slots", {"id": svc})))
code, out = call(bob, "cancel_booking", {"id": booked["id"]})
check("a cancelled booking cannot be cancelled again", code != 200, (code, out))
again = ok(cy, "book", {"id": free_one["id"]})
ok(owner, "cancel_booking", {"id": again["id"]})
check("the host can cancel too", all(b["id"] != again["id"] for b in ok(cy, "my_bookings")))

# --- switching off, breaks, editing
ok(owner, "update_service", {"id": svc, "buffer": "15", "title": "Office hours " + RUN})
padded = ok(bob, "free_slots", {"id": svc})
pad_taken = {s["starts_at"] for s in after} - {s["starts_at"] for s in padded}
check("a break after meetings keeps the neighbours of busy time free", len(pad_taken) >= 2, pad_taken)
ok(owner, "update_service", {"id": svc, "active": False})
code, out = call(bob, "free_slots", {"id": svc})
check("a switched-off service offers nothing", code != 200, (code, out))
check("and is not listed", all(s["id"] != svc for s in ok(bob, "find_services")))
ok(owner, "delete_service", {"id": svc})
check("a service without upcoming bookings can be deleted", all(s["id"] != svc for s in ok(owner, "my_services")))
ok(owner, "delete_event", {"id": ev}, "atrium_calendar")

finish()
