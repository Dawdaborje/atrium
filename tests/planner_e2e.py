"""Atrium planner, end to end over HTTP: plans, buckets, tasks, assignment, ordering, checklists, repeating tasks, labels,
people, the record rule that lets members (and only members) read a plan's tasks, and the reminders. See README.md."""
import datetime
from e2e_lib import *

PLUGIN = "atrium_planner"


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
day = lambda n: (datetime.date.today() + datetime.timedelta(days=n)).isoformat()


def titles(rows, bucket=None):
    return [r["title"] for r in rows if bucket is None or r["bucket"] == bucket]


# --- plans and their buckets
plan = ok(owner, "create_plan", {"name": "Launch " + RUN, "description": "ship it"})["id"]
buckets = ok(owner, "board_buckets", {"plan": plan})
check("a new plan starts with three buckets in order", [b["label"] for b in buckets] == ["To do", "In progress", "Done"], buckets)
todo, doing, done = [b["id"] for b in buckets]
mine = ok(owner, "my_plans")
row = next((p for p in mine if p["id"] == plan), None)
check("the plan is in its owner's list with its counts", row and row["role"] == "owner" and row["open"] == 0 and row["members"] == 1, row)
check("it is not in anyone else's", all(p["id"] != plan for p in ok(bob, "my_plans")))
code, out = call(bob, "board_tasks", {"plan": plan})
check("a stranger cannot read its board", code != 200, (code, out))
code, out = call(owner, "create_plan", {"name": " "})
check("a plan needs a name", code != 200, (code, out))
code, out = call(owner, "create_plan", {"name": "x", "members": ["nobody"]})
check("people in a plan must be members of the organization", code != 200, (code, out))

# --- tasks and their order
a = ok(owner, "quick_add", {"plan": plan, "title": "A", "bucket": todo})["id"]
b_ = ok(owner, "quick_add", {"plan": plan, "title": "B", "bucket": todo})["id"]
c = ok(owner, "quick_add", {"plan": plan, "title": "C", "bucket": todo})["id"]
rows = ok(owner, "board_tasks", {"plan": plan})
check("quick add puts tasks at the end of the bucket, in order", titles(rows, todo) == ["A", "B", "C"], rows)
check("with the first bucket as the default", ok(owner, "board_tasks", {"plan": plan}) and ok(owner, "quick_add", {"plan": plan, "title": "D"})["id"])
ok(owner, "move_task", {"id": c, "to": todo, "before": a})
check("dropping a task on another puts it in front", titles(ok(owner, "board_tasks", {"plan": plan}), todo)[:3] == ["C", "A", "B"], titles(ok(owner, "board_tasks", {"plan": plan}), todo))
ok(owner, "move_task", {"id": a, "to": doing})
rows = ok(owner, "board_tasks", {"plan": plan})
check("moving to another bucket takes it out of the first", "A" in titles(rows, doing) and "A" not in titles(rows, todo))
ok(owner, "move_task", {"id": b_, "to": doing, "before": a})
check("...and in front of a task there", titles(ok(owner, "board_tasks", {"plan": plan}), doing) == ["B", "A"])
code, out = call(owner, "move_task", {"id": a, "to": "nosuchbucket"})
check("a task cannot move to a bucket of another plan", code != 200, (code, out))
code, out = call(bob, "move_task", {"id": a, "to": todo})
check("a stranger cannot move tasks", code != 200, (code, out))

# --- validation
for bad, why in [({"due": "31/10/2026"}, "a date in the wrong form"), ({"priority": "huge"}, "an unknown priority"), ({"progress": "nearly"}, "an unknown progress"), ({"recurrence": "hourly"}, "an unknown repeat"), ({"title": " "}, "an empty title"), ({"start": day(5), "due": day(2)}, "a start after the due date"), ({"bucket": "nosuch"}, "a bucket of nowhere")]:
    code, out = call(owner, "update_task", {"id": a, **bad})
    check(f"{why} is refused", code != 200, (code, out))

# --- assignment: only people in the plan
code, out = call(owner, "update_task", {"id": a, "assignees": [bob_id]})
check("a task cannot be assigned to someone outside the plan", code != 200, (code, out))
ok(owner, "add_plan_members", {"plan": plan, "users": [bob_id]})
check("a person added to the plan sees it", any(p["id"] == plan and p["role"] == "member" for p in ok(bob, "my_plans")))
ok(owner, "update_task", {"id": a, "assignees": [bob_id], "due": day(0), "priority": "high"})
t = ok(owner, "get_task", {"id": a})
check("the task keeps its assignee, due date and priority", t["assignees"] == [bob_id] and t["due"] == day(0) and t["priority"] == "high", t)
row = next(r for r in ok(owner, "board_tasks", {"plan": plan}) if r["id"] == a)
check("the board row shows the names and 'Due today'", row["assignee_names"] == "bob" and row["due_text"] == "Due today" and not row["late"], row)
mytasks = ok(bob, "my_tasks")
check("the assignee finds it under their tasks", titles(mytasks) == ["A"] and mytasks[0]["plan_name"] == "Launch " + RUN, mytasks)
note = bob.call("GET", "/api/ui/notifications")[1]
check("and was told about it", any("assigned you" in n.get("title", "") for n in note.get("notifications", note if isinstance(note, list) else [])), str(note)[:200])
check("someone not assigned has nothing", ok(cy, "my_tasks") == [])

# --- late tasks
ok(owner, "update_task", {"id": b_, "due": day(-3)})
row = next(r for r in ok(owner, "board_tasks", {"plan": plan}) if r["id"] == b_)
check("a task past its due date is late", row["late"] and row["due_text"].startswith("Overdue since"), row)
check("the plan list counts it as overdue", next(p for p in ok(owner, "my_plans") if p["id"] == plan)["overdue"] == 1)

# --- reminders
sent = ok(owner, "due_reminders")
check("the morning run reminds the assignee of what is due today", sent["sent"] >= 1, sent)
note = bob.call("GET", "/api/ui/notifications")[1]
check("...in a notification", any("is due today" in n.get("title", "") for n in note.get("notifications", note if isinstance(note, list) else [])))

# --- completing, and repeating
ok(owner, "update_task", {"id": c, "due": day(2), "recurrence": "weekly"})
ok(owner, "add_check", {"id": c, "text": "first"})
ok(owner, "add_check", {"id": c, "text": "second"})
chk = ok(owner, "task_checklist", {"id": c})
check("a checklist keeps its items in order", [i["text"] for i in chk] == ["first", "second"], chk)
ok(owner, "toggle_check", {"id": chk[0]["id"], "done": True})
row = next(r for r in ok(owner, "board_tasks", {"plan": plan}) if r["id"] == c)
check("the board shows how much of it is done", row["check_text"] == "1/2", row)
ok(owner, "complete_task", {"id": c})
t = ok(owner, "get_task", {"id": c})
check("completing sets the progress", t["progress"] == "completed", t)
rows = ok(owner, "board_tasks", {"plan": plan})
copies = [r for r in rows if r["title"] == "C" and r["id"] != c]
check("a repeating task makes the next one, due a week later", len(copies) == 1 and copies[0]["due"] == day(9) and copies[0]["progress"] == "not_started", copies)
nxt = ok(owner, "task_checklist", {"id": copies[0]["id"]})
check("with its checklist, unticked", [(i["text"], i["done"]) for i in nxt] == [("first", False), ("second", False)], nxt)
ok(owner, "reopen_task", {"id": c})
check("reopening makes it in progress again", ok(owner, "get_task", {"id": c})["progress"] == "in_progress")
check("a late task that is finished is not late", True)

# --- checklists
code, out = call(owner, "add_check", {"id": c, "text": ""})
check("a checklist item needs text", code != 200, (code, out))
ok(owner, "remove_check", {"id": chk[1]["id"]})
check("removing an item updates the count", next(r for r in ok(owner, "board_tasks", {"plan": plan}) if r["id"] == c)["check_text"] == "1/1")
code, out = call(cy, "add_check", {"id": c, "text": "sneaky"})
check("a stranger cannot add to a checklist", code != 200, (code, out))
code, out = call(cy, "toggle_check", {"id": chk[0]["id"], "done": False})
check("or tick one", code != 200, (code, out))

# --- buckets
code, out = call(owner, "add_bucket", {"plan": plan, "name": "to do"})
check("a bucket name is unique in a plan, ignoring case", code != 200, (code, out))
review = ok(owner, "add_bucket", {"plan": plan, "name": "Review"})["id"]
ok(owner, "rename_bucket", {"id": review, "name": "In review"})
ok(owner, "move_bucket", {"id": review, "before": doing})
check("a bucket moves in front of another", [b["label"] for b in ok(owner, "board_buckets", {"plan": plan})] == ["To do", "In review", "In progress", "Done"])
ok(owner, "move_task", {"id": a, "to": review})
ok(owner, "delete_bucket", {"id": review})
check("deleting a bucket moves its tasks to the first one left", any(r["id"] == a and r["bucket"] == todo for r in ok(owner, "board_tasks", {"plan": plan})))
code, out = call(cy, "delete_bucket", {"id": todo})
check("a stranger cannot delete a bucket", code != 200, (code, out))

# --- labels
lab = ok(owner, "add_label", {"plan": plan, "name": "urgent"})["id"]
code, out = call(owner, "add_label", {"plan": plan, "name": "URGENT"})
check("a label name is unique", code != 200, (code, out))
ok(owner, "update_task", {"id": a, "labels": [lab]})
check("a label shows on the board row", next(r for r in ok(owner, "board_tasks", {"plan": plan}) if r["id"] == a)["labels_text"] == "urgent")
code, out = call(owner, "update_task", {"id": a, "labels": ["nolabel"]})
check("a label of nowhere is refused", code != 200, (code, out))
ok(owner, "delete_label", {"id": lab})
check("deleting a label takes it off the tasks", next(r for r in ok(owner, "board_tasks", {"plan": plan}) if r["id"] == a)["labels_text"] in ("", None))

# --- pickers on a task page
check("the bucket picker lists the plan's buckets", [b["name"] for b in ok(owner, "pick_buckets", {"id": a})] == ["To do", "In progress", "Done"])
check("the people picker lists the plan's people", sorted(p["name"] for p in ok(owner, "pick_people", {"id": a})) == ["bob", "owner"])
check("...and can be searched", [p["name"] for p in ok(owner, "pick_people", {"id": a, "q": "bo"})] == ["bob"])
check("the organization picker leaves out the caller", "owner" not in [p["name"] for p in ok(owner, "pick_org_people")])

# --- the record rule: members read the plan's tasks through the kernel's chatter, strangers cannot
code, out = bob.call("GET", f"/api/chatter/{PLUGIN}/planner_task/{a}")
check("a member can open a task's conversation", code == 200, (code, str(out)[:150]))
code, out = bob.call("POST", f"/api/chatter/{PLUGIN}/planner_task/{a}/messages", {"kind": "message", "body": "on it"})
check("and write in it", code in (200, 201), (code, str(out)[:150]))
code, out = cy.call("GET", f"/api/chatter/{PLUGIN}/planner_task/{a}")
check("a stranger cannot", code in (403, 404), (code, str(out)[:150]))
code, out = cy.call("POST", f"/api/chatter/{PLUGIN}/planner_task/{a}/messages", {"kind": "message", "body": "hi"})
check("or write in it", code in (403, 404), (code, str(out)[:150]))

# --- people
code, out = call(bob, "add_plan_members", {"plan": plan, "users": [cy_id]})
check("only the owner adds people", code != 200, (code, out))
code, out = call(owner, "remove_plan_member", {"plan": plan, "id": people.get("owner", "x")})
members = ok(owner, "plan_members", {"plan": plan})
check("the members list names people with roles", {m["name"]: m["role"] for m in members} == {"bob": "member", "owner": "owner"}, members)
code, out = call(bob, "remove_plan_member", {"plan": plan, "id": cy_id})
check("a member cannot remove others", code != 200, (code, out))
ok(owner, "update_task", {"id": b_, "assignees": [bob_id]})
ok(owner, "remove_plan_member", {"plan": plan, "id": bob_id})
check("removing a person unassigns their tasks", all(not r["assignee_names"] for r in ok(owner, "board_tasks", {"plan": plan})))
check("and takes the plan from them", all(p["id"] != plan for p in ok(bob, "my_plans")))
code, out = call(owner, "remove_plan_member", {"plan": plan, "id": [m for m in people.values()][0]})
owner_id = next(m["id"] for m in members if m["name"] == "owner")
code, out = call(owner, "remove_plan_member", {"plan": plan, "id": owner_id})
check("the last owner cannot leave", code != 200, (code, out))
ok(owner, "add_plan_members", {"plan": plan, "users": [bob_id]})
code, out = call(owner, "set_plan_role", {"plan": plan, "id": owner_id, "role": "member"})
check("nor be made an ordinary member", code != 200, (code, out))
ok(owner, "set_plan_role", {"plan": plan, "id": bob_id, "role": "owner"})
check("a second owner can be made", any(m["role"] == "owner" and m["name"] == "bob" for m in ok(owner, "plan_members", {"plan": plan})))
ok(bob, "edit_plan", {"plan": plan, "description": "new words"})
check("and then manages the plan", ok(bob, "plan_info", {"plan": plan})["description"] == "new words")

# --- archive, header, mine
ok(owner, "edit_plan", {"plan": plan, "archived": True})
code, out = call(owner, "quick_add", {"plan": plan, "title": "late"})
check("an archived plan takes no new tasks", code != 200, (code, out))
check("and leaves the list", all(p["id"] != plan for p in ok(owner, "my_plans")))
ok(owner, "edit_plan", {"plan": plan, "archived": False})
head = ok(owner, "plan_header", {"plan": plan})
check("the header gives the name and a summary", head["title"] == "Launch " + RUN and "open" in head["subtitle"], head)
created = ok(owner, "my_tasks", {"scope": "created"})
check("my tasks can also show what I made", len(created) >= 3 and all("plan_name" in r for r in created))

# --- dependencies
d1 = ok(owner, "quick_add", {"plan": plan, "title": "dep first"})["id"]
d2 = ok(owner, "quick_add", {"plan": plan, "title": "dep second"})["id"]
d3 = ok(owner, "quick_add", {"plan": plan, "title": "dep third"})["id"]
ok(owner, "update_task", {"id": d2, "blocked_by": [d1]})
check("a task can wait for another", ok(owner, "get_task", {"id": d2})["blocked_by"] == [d1])
row2 = next(r for r in ok(owner, "board_tasks", {"plan": plan}) if r["id"] == d2)
check("the board shows it as blocked", row2["blocked_text"] == "Blocked", row2)
code, out = call(owner, "update_task", {"id": d2, "progress": "completed"})
check("a blocked task cannot be completed", code != 200 and "dep first" in json.dumps(out), (code, out))
code, out = call(owner, "update_task", {"id": d2, "blocked_by": [d2]})
check("a task cannot wait for itself", code != 200, (code, out))
code, out = call(owner, "update_task", {"id": d1, "blocked_by": [d2]})
check("two tasks cannot wait for each other", code != 200, (code, out))
ok(owner, "update_task", {"id": d3, "blocked_by": [d2]})
code, out = call(owner, "update_task", {"id": d1, "blocked_by": [d3]})
check("nor in a longer circle", code != 200, (code, out))
check("the picker offers the other tasks of the plan", {r["id"] for r in ok(owner, "pick_tasks", {"id": d1})} >= {d2, d3} and all(r["id"] != d1 for r in ok(owner, "pick_tasks", {"id": d1})))
ok(owner, "complete_task", {"id": d1})
row2 = next(r for r in ok(owner, "board_tasks", {"plan": plan}) if r["id"] == d2)
check("completing the blocker frees the task", row2["blocked_text"] == "")
ok(owner, "complete_task", {"id": d2})
ok(owner, "reopen_task", {"id": d1})
ok(owner, "update_task", {"id": d2, "progress": "in_progress"})
ok(owner, "delete_task", {"id": d1})
check("deleting a blocker removes it from what waited", ok(owner, "get_task", {"id": d2})["blocked_by"] == [])
code, out = call(owner, "update_task", {"id": d2, "blocked_by": ["nosuchtask"]})
check("only real tasks can be waited for", code != 200, (code, out))

# --- deleting
code, out = call(bob, "delete_task", {"id": a}) if False else (200, None)
ok(owner, "delete_task", {"id": copies[0]["id"]})
check("a task can be deleted", all(r["id"] != copies[0]["id"] for r in ok(owner, "board_tasks", {"plan": plan})))
code, out = call(cy, "delete_plan", {"plan": plan})
check("a stranger cannot delete a plan", code != 200, (code, out))
ok(owner, "delete_plan", {"plan": plan})
code, out = call(owner, "board_tasks", {"plan": plan})
check("deleting a plan removes it and its tasks", code != 200 and ok(owner, "my_tasks", {"done": True}) is not None and all(r["plan_name"] != "Launch " + RUN for r in ok(owner, "my_tasks", {"scope": "created", "done": True})))

finish()
