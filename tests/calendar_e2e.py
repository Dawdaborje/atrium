"""Atrium calendar, end to end over HTTP: personal and shared calendars, roles, events that repeat and leave out days,
invitations and answers, clashes, the agenda and availability, iCalendar files, and the reminders. See README.md."""
import datetime
from e2e_lib import *

PLUGIN = "atrium_calendar"


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
people["owner"] = {p["name"]: p["id"] for p in bob.call("POST", "/api/plugins/atrium_chat/list_people", {})[1]}["owner"]
today = datetime.date.today()
day = lambda n: (today + datetime.timedelta(days=n)).isoformat()
stamp = lambda n, hm: f"{day(n)}T{hm}:00Z"
window = lambda a, b: {"start": stamp(a, "00:00"), "end": stamp(b, "23:59")}


def titles(rows):
    return [r["title"] for r in rows]


# --- every person has a calendar of their own
mine = ok(owner, "my_calendars")
check("a person has a calendar of their own", len(mine) == 1 and mine[0]["personal"] and mine[0]["role"] == "owner", mine)
own = mine[0]["id"]
check("the same one comes back the next time", ok(owner, "my_calendars")[0]["id"] == own)
check("someone else's is not in the list", ok(bob, "my_calendars")[0]["id"] != own)
code, out = call(bob, "calendar_info", {"calendar": own})
check("a stranger cannot read it", code != 200, (code, out))
code, out = call(owner, "delete_calendar", {"calendar": own})
check("a personal calendar cannot be deleted", code != 200, (code, out))

# --- events
ev = ok(owner, "create_event", {"title": "Review " + RUN, "starts_at": stamp(1, "09:00"), "ends_at": stamp(1, "10:00"), "location": "HQ"})["id"]
rows = ok(owner, "events_in", window(0, 3))
row = next((r for r in rows if r["id"] == ev), None)
check("an event lands in the owner's calendar and the month view", row and row["calendar"] == own and row["location"] == "HQ", rows)
check("its time reads naturally", row and ", 09:00 – 10:00" in row["when_text"], row)
check("it is outside a window that misses it", all(r["id"] != ev for r in ok(owner, "events_in", window(5, 9))))
check("others do not see it", all(r["id"] != ev for r in ok(bob, "events_in", window(0, 3))))
code, out = call(bob, "get_event", {"id": ev})
check("a stranger cannot open it", code != 200, (code, out))
code, out = call(owner, "create_event", {"title": " ", "starts_at": stamp(1, "09:00")})
check("an event needs a title", code != 200, (code, out))
code, out = call(owner, "create_event", {"title": "Backwards", "starts_at": stamp(1, "10:00"), "ends_at": stamp(1, "09:00")})
check("an event cannot end before it starts", code != 200, (code, out))
code, out = call(owner, "create_event", {"title": "Odd", "starts_at": "tomorrow"})
check("a start must be a date and time", code != 200, (code, out))
form = ok(owner, "create_event", {"title": "Form style " + RUN, "starts_at": f"{day(1)}T14:30", "ends_at": f"{day(1)}T15:00"})["id"]
check("times typed on a page are accepted", any(r["id"] == form for r in ok(owner, "events_in", window(1, 1))))
trip = ok(owner, "create_event", {"title": "Offsite " + RUN, "starts_at": day(2), "ends_at": day(3), "all_day": True})["id"]
one = next(r for r in ok(owner, "events_in", window(0, 5)) if r["id"] == trip)
check("an all-day event covers whole days", one["all_day"] and one["starts_at"] == stamp(2, "00:00") and one["ends_at"] == f"{day(3)}T23:59:59Z", one)
ok(owner, "update_event", {"id": ev, "title": "Review moved " + RUN, "location": "Room 2"})
got = ok(owner, "get_event", {"id": ev})
check("an event can be changed", got["title"] == "Review moved " + RUN and got["location"] == "Room 2" and got["can_edit"], got)

# --- repeating events
rep = ok(owner, "create_event", {"title": "Stand-up " + RUN, "starts_at": stamp(0, "08:00"), "ends_at": stamp(0, "08:15"), "repeat": "daily", "until": day(4)})["id"]
seen = [r for r in ok(owner, "events_in", window(0, 10)) if r["id"] == rep]
check("a daily event shows each day until it ends", [r["day"] for r in seen] == [day(i) for i in range(5)], [r["day"] for r in seen])
ok(owner, "update_event", {"id": rep, "skip": day(2)})
seen = [r["day"] for r in ok(owner, "events_in", window(0, 10)) if r["id"] == rep]
check("a day can be left out", day(2) not in seen and len(seen) == 4, seen)
code, out = call(owner, "update_event", {"id": ev, "skip": day(1)})
check("only a repeating event has days to leave out", code != 200, (code, out))
code, out = call(owner, "create_event", {"title": "Bad rule", "starts_at": stamp(2, "08:00"), "repeat": "weekly", "until": day(-3)})
check("a repeat cannot end before it starts", code != 200, (code, out))
weekly = ok(owner, "create_event", {"title": "Weekly " + RUN, "starts_at": stamp(-14, "11:00"), "repeat": "weekly"})["id"]
seen = [r for r in ok(owner, "events_in", window(-1, 14)) if r["id"] == weekly]
check("a weekly event that began long ago shows in a later window", len(seen) == 2 or len(seen) == 3, seen)
code, out = call(owner, "events_in", {"start": stamp(0, "00:00"), "end": stamp(900, "00:00")})
check("a window is at most 400 days", code != 200, (code, out))

# --- shared calendars and roles
cal = ok(owner, "create_calendar", {"name": "Team " + RUN, "members": [bob_id]})["id"]
check("a shared calendar is in its members' lists", any(c["id"] == cal and c["role"] == "editor" for c in ok(bob, "my_calendars")))
check("and not in a stranger's", all(c["id"] != cal for c in ok(cy, "my_calendars")))
shared = ok(bob, "create_event", {"title": "Planning " + RUN, "calendar": cal, "starts_at": stamp(1, "13:00"), "ends_at": stamp(1, "14:00")})["id"]
check("an editor adds events to a shared calendar", any(r["id"] == shared for r in ok(owner, "events_in", window(0, 3))))
check("every member sees them", any(r["id"] == shared for r in ok(bob, "events_in", window(0, 3))))
check("a stranger does not", all(r["id"] != shared for r in ok(cy, "events_in", window(0, 3))))
check("one calendar can be viewed alone", titles(ok(owner, "events_in", {**window(0, 3), "calendar": cal})) == ["Planning " + RUN])
code, out = call(cy, "events_in", {**window(0, 3), "calendar": cal})
check("a stranger cannot ask for it", code != 200, (code, out))
code, out = call(cy, "create_event", {"title": "Sneaky", "calendar": cal, "starts_at": stamp(1, "13:00")})
check("a stranger cannot add to it", code != 200, (code, out))
ok(owner, "add_calendar_members", {"calendar": cal, "users": [cy_id], "role": "viewer"})
check("a viewer sees the events", any(r["id"] == shared for r in ok(cy, "events_in", window(0, 3))))
code, out = call(cy, "create_event", {"title": "Viewer", "calendar": cal, "starts_at": stamp(1, "13:00")})
check("a viewer cannot add events", code != 200, (code, out))
code, out = call(cy, "update_event", {"id": shared, "title": "Mine now"})
check("a viewer cannot change events", code != 200, (code, out))
code, out = call(cy, "delete_event", {"id": shared})
check("a viewer cannot delete events", code != 200, (code, out))
code, out = call(bob, "add_calendar_members", {"calendar": cal, "users": [cy_id]})
check("only an owner adds people", code != 200, (code, out))
code, out = call(bob, "edit_calendar", {"calendar": cal, "name": "Hijack"})
check("only an owner renames", code != 200, (code, out))
ok(owner, "set_calendar_role", {"calendar": cal, "id": cy_id, "role": "editor"})
ok(cy, "update_event", {"id": shared, "location": "Cy's room"})
check("a viewer made an editor can change events", ok(cy, "get_event", {"id": shared})["location"] == "Cy's room")
code, out = call(owner, "set_calendar_role", {"calendar": cal, "id": people["owner"], "role": "viewer"})
check("the last owner cannot step down", code != 200, (code, out))
code, out = call(owner, "remove_calendar_member", {"calendar": cal, "id": people["owner"]})
check("the last owner cannot leave", code != 200, (code, out))
ok(cy, "remove_calendar_member", {"calendar": cal, "id": cy_id})
check("anyone may leave a calendar", all(c["id"] != cal for c in ok(cy, "my_calendars")))
check("members list shows names and roles", {m["name"]: m["role"] for m in ok(owner, "calendar_members", {"calendar": cal})} == {"owner": "owner", "bob": "editor"})
code, out = call(bob, "delete_calendar", {"calendar": cal})
check("only an owner deletes a calendar", code != 200, (code, out))

# --- invitations
inv = ok(owner, "create_event", {"title": "Kick-off " + RUN, "starts_at": stamp(2, "10:00"), "ends_at": stamp(2, "11:00"), "attendees": [bob_id, cy_id]})["id"]
check("an invited person sees the event without being in the calendar", any(r["id"] == inv and r["response"] == "needs_action" for r in ok(bob, "events_in", window(0, 4))))
check("and can open it", ok(bob, "get_event", {"id": inv})["my_response"] == "needs_action")
check("the owner's own row has no answer", all(r["response"] == "" for r in ok(owner, "events_in", window(0, 4)) if r["id"] == inv))
ok(bob, "respond", {"id": inv, "status": "accepted"})
check("an answer is kept", ok(bob, "get_event", {"id": inv})["my_response"] == "accepted")
check("the owner sees who answered", "bob: accepted" in ok(owner, "get_event", {"id": inv})["responses"] and "cy: needs action" in ok(owner, "get_event", {"id": inv})["responses"])
code, out = call(owner, "respond", {"id": inv, "status": "accepted"})
check("only the invited can answer", code != 200, (code, out))
code, out = call(bob, "respond", {"id": inv, "status": "perhaps"})
check("an answer is accepted, tentative or declined", code != 200, (code, out))
code, out = call(bob, "update_event", {"id": inv, "title": "Changed by bob"})
check("an invited person cannot change the event", code != 200, (code, out))
ok(bob, "update_event", {"id": inv, "my_response": "declined"})
check("an answer can be given through the form", ok(bob, "get_event", {"id": inv})["my_response"] == "declined")
check("a declined event leaves the invitee's calendar", all(r["id"] != inv for r in ok(bob, "events_in", window(0, 4))))
code, out = call(owner, "create_event", {"title": "Ghost", "starts_at": stamp(2, "10:00"), "attendees": ["nobody"]})
check("only members of the organization can be invited", code != 200, (code, out))
ok(owner, "update_event", {"id": inv, "starts_at": stamp(2, "15:00"), "ends_at": stamp(2, "16:00")})
check("moving an event clears the answers", ok(cy, "get_event", {"id": inv})["my_response"] == "needs_action")
ok(owner, "update_event", {"id": inv, "attendees": [cy_id]})
check("someone taken off an event no longer sees it", all(r["id"] != inv for r in ok(bob, "events_in", window(0, 4))))
code, out = call(bob, "get_event", {"id": inv})
check("nor opens it", code != 200, (code, out))

# --- clashes and availability
first = ok(owner, "create_event", {"title": "Call A " + RUN, "starts_at": stamp(3, "09:00"), "ends_at": stamp(3, "10:00"), "attendees": [bob_id]})
check("an event with nobody in conflict reports none", first["conflicts"] == [], first)
second = ok(cy, "create_event", {"title": "Call B " + RUN, "starts_at": stamp(3, "09:30"), "ends_at": stamp(3, "10:30"), "attendees": [bob_id]})
check("inviting someone who is busy reports the clash", second["conflicts"] == ["bob"], second)
check("the form says so too", "bob" in ok(cy, "get_event", {"id": second["id"]})["busy"])
free = ok(cy, "create_event", {"title": "Call C " + RUN, "starts_at": stamp(3, "12:00"), "ends_at": stamp(3, "13:00"), "attendees": [bob_id]})
check("a free slot has no clash", free["conflicts"] == [], free)
av = ok(owner, "availability")
check("availability lists the organization", {"owner", "bob", "cy"} <= {r["name"] for r in av} and all(r["status"] in ("free", "busy") for r in av), av)
check("and when each person is next busy", any(r["name"] == "bob" and r["next_busy"] != "nothing this week" for r in av), av)
check("never what the events are", all("Call" not in json.dumps(r) for r in av))
ag = ok(owner, "agenda")
check("the agenda lists the next two weeks in order", titles(ag)[:1] != [] and [r["starts_at"] for r in ag] == sorted(r["starts_at"] for r in ag), ag[:3])

# --- files
before = len(ok(owner, "events_in", window(-20, 30)))
out = ok(owner, "export_ics", {"calendar": own})
text = out["ics"]
check("a calendar exports as an iCalendar file", text.startswith("BEGIN:VCALENDAR") and "SUMMARY:Review moved " + RUN in text and "RRULE:FREQ=DAILY" in text and "EXDATE:" in text, text[:200])
check("it keeps all-day events whole days", "DTSTART;VALUE=DATE:" + day(2).replace("-", "") in text and "DTEND;VALUE=DATE:" + day(4).replace("-", "") in text)
code, out2 = call(cy, "export_ics", {"calendar": own})
check("a stranger cannot export it", code != 200, (code, out2))
target = ok(owner, "create_calendar", {"name": "Imported " + RUN})["id"]
res = ok(owner, "import_ics", {"calendar": target, "ics": text})
check("it imports into another calendar", res["imported"] == out["events"] and res["skipped"] == 0, res)
check("with the same events, repeats and gaps", len(ok(owner, "events_in", {**window(-20, 30), "calendar": target})) == before, (before,))
code, out2 = call(owner, "import_ics", {"calendar": target, "ics": "not a calendar"})
check("a file with no events is refused", code != 200, (code, out2))
code, out2 = call(cy, "import_ics", {"calendar": target, "ics": text})
check("a stranger cannot import", code != 200, (code, out2))
res = ok(owner, "import_ics", {"calendar": target, "ics": "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nDTSTART:20261201T090000Z\r\nSUMMARY:Biweekly\r\nRRULE:FREQ=WEEKLY;INTERVAL=2\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nSUMMARY:No start\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n"})
check("a rule that is not kept is simplified, and a broken event skipped", res == {"imported": 1, "skipped": 1, "rules_simplified": 1}, res)

# --- reminders run
check("the morning agenda runs", ok(owner, "morning_agenda")["sent"] >= 1)
check("the reminder before an event runs", "sent" in ok(owner, "starting_soon"))

# --- time zones
check("nobody has a zone to begin with", ok(bob, "my_timezone")["tz"] == "")
code, out = call(bob, "save_timezone", {"tz": "Mars/Base"})
check("a zone must be a real one", code != 200, (code, out))
check("zones can be looked up", "Europe/Oslo" in [z["id"] for z in ok(bob, "pick_timezones", {"q": "oslo"})])
check("a zone is saved by its proper name", ok(bob, "save_timezone", {"tz": "europe/oslo"})["tz"] == "Europe/Oslo" and ok(bob, "my_timezone")["tz"] == "Europe/Oslo")
tz_ev = ok(bob, "create_event", {"title": "Zoned " + RUN, "starts_at": "2099-07-01T09:00", "ends_at": "2099-07-01T10:00", "attendees": [cy_id]})["id"]
mine = ok(bob, "get_event", {"id": tz_ev})
check("the creator reads the time they typed", mine["starts_at"].startswith("2099-07-01T09:00") and not mine["starts_at"].endswith("Z") and mine["time_zone"] == "Europe/Oslo", mine)
theirs = ok(cy, "get_event", {"id": tz_ev})
check("a person without a zone reads it in UTC, two hours earlier in summer", theirs["starts_at"] == "2099-07-01T07:00:00Z" and theirs["time_zone"] == "UTC", theirs)
ok(cy, "save_timezone", {"tz": "Asia/Tokyo"})
check("a person in another zone reads it there", ok(cy, "get_event", {"id": tz_ev})["starts_at"].startswith("2099-07-01T16:00"))
row = next(r for r in ok(bob, "events_in", {"start": "2099-06-30T00:00:00Z", "end": "2099-07-03T00:00:00Z"}) if r["id"] == tz_ev)
check("the month grid and its text use the reader's zone", row["starts_at"].startswith("2099-07-01T09:00") and "09:00" in row["when_text"] and row["day"] == "2099-07-01", row)
ok(bob, "update_event", {"id": tz_ev, "starts_at": "2099-07-01T10:30", "ends_at": "2099-07-01T11:00"})
check("changing the time is read in the changer's zone", ok(cy, "get_event", {"id": tz_ev})["starts_at"].startswith("2099-07-01T17:30"))
check("typing a time with a Z is taken as UTC", ok(bob, "typed_to_utc", {"value": "2099-07-01T09:00"})["utc"] == "2099-07-01T07:00:00Z" and ok(bob, "typed_to_utc", {"value": "2099-07-01T09:00:00Z"})["utc"] == "2099-07-01T09:00:00Z")
check("other plugins can read stored times for the caller", ok(bob, "read_times", {"values": ["2099-07-01T07:00:00Z", "soon"]}) == ["2099-07-01 09:00", "soon"])
# A weekly event made in Oslo before the clocks go back stays at 09:00 local afterwards.
wk = ok(bob, "create_event", {"title": "Weekly zoned " + RUN, "starts_at": "2099-10-20T09:00", "ends_at": "2099-10-20T09:30", "repeat": "weekly"})["id"]
occ = [r for r in ok(bob, "events_in", {"start": "2099-10-18T00:00:00Z", "end": "2099-11-08T00:00:00Z"}) if r["id"] == wk]
check("a repeating event keeps its local time through the clock change", len(occ) >= 3 and all(r["starts_at"].split("T")[1].startswith("09:00") for r in occ), [r["starts_at"] for r in occ])
raw = ok(cy, "save_timezone", {"tz": ""})
check("a zone can be cleared", raw["tz"] == "" and ok(cy, "my_timezone")["tz"] == "")
ok(bob, "save_timezone", {"tz": ""})
ok(bob, "delete_event", {"id": tz_ev})
ok(bob, "delete_event", {"id": wk})

# --- deleting
ok(owner, "delete_occurrence", {"id": rep, "date": day(3)})
check("one occurrence can be deleted", day(3) not in [r["day"] for r in ok(owner, "events_in", window(0, 10)) if r["id"] == rep])
ok(owner, "delete_event", {"id": ev})
code, out = call(owner, "get_event", {"id": ev})
check("a deleted event is gone", code != 200, (code, out))
ok(owner, "delete_calendar", {"calendar": cal})
check("deleting a calendar takes its events", all(r["id"] != shared for r in ok(owner, "events_in", window(0, 3))) and all(c["id"] != cal for c in ok(bob, "my_calendars")))
finish()
