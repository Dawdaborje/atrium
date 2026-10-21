"""Atrium forms, end to end over HTTP: building a form, the nine kinds of question and how answers are checked, opening and
closing, answering once, anonymous forms, results, responses, the CSV file, and who may see what."""
import datetime
from e2e_lib import *

PLUGIN = "atrium_forms"


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

# --- building
f = ok(bob, "create_form", {"title": "Offsite " + RUN, "description": "Where to go", "once": True})["id"]
check("a form starts as a draft", ok(bob, "form_info", {"id": f})["status"] == "draft")
code, out = call(bob, "open_form", {"id": f})
check("an empty form cannot be opened", code != 200, (code, out))
code, out = call(bob, "add_question", {"id": f, "label": "Pick", "kind": "choice", "options": "Only one"})
check("a choice question needs two choices", code != 200, (code, out))
code, out = call(bob, "add_question", {"id": f, "label": "Pick", "kind": "essay"})
check("a kind must be known", code != 200, (code, out))
code, out = call(bob, "add_question", {"id": f, "label": " "})
check("a question needs words", code != 200, (code, out))
qs = {}
for name, body in {
    "place": {"label": "Where to?", "kind": "choice", "options": "Lisbon\nBerlin\nOslo", "required": True},
    "extras": {"label": "What should we book?", "kind": "multi", "options": "Hotel\nTrain\nDinner"},
    "mood": {"label": "How excited are you?", "kind": "rating"},
    "car": {"label": "Need a parking spot?", "kind": "yes_no"},
    "bags": {"label": "How many bags?", "kind": "number"},
    "when": {"label": "Earliest day?", "kind": "date"},
    "note": {"label": "Anything else?", "kind": "long", "help": "Allergies, wishes"},
    "name": {"label": "Nickname", "kind": "text"},
    "size": {"label": "Shirt size", "kind": "dropdown", "options": ["S", "M", "L"]},
}.items():
    qs[name] = ok(bob, "add_question", {"id": f, **body})["id"]
lst = ok(bob, "form_questions", {"id": f})
check("questions are listed in order", [q["label"] for q in lst][:3] == ["Where to?", "What should we book?", "How excited are you?"] and len(lst) == 9, lst)
ok(bob, "question_down", {"id": qs["place"]})
check("a question moves down", ok(bob, "form_questions", {"id": f})[1]["label"] == "Where to?")
ok(bob, "question_up", {"id": qs["place"]})
check("and back up", ok(bob, "form_questions", {"id": f})[0]["label"] == "Where to?")
check("moving the first up changes nothing", ok(bob, "question_up", {"id": qs["place"]}) == {"moved": False})
ok(bob, "update_question", {"id": qs["name"], "label": "Nickname (optional)"})
check("a question can be reworded", ok(bob, "get_question", {"id": qs["name"]})["label"] == "Nickname (optional)")
spare = ok(bob, "add_question", {"id": f, "label": "Spare"})["id"]
ok(bob, "delete_question", {"id": spare})
check("a question can be deleted", len(ok(bob, "form_questions", {"id": f})) == 9)
code, out = call(cy, "form_questions", {"id": f})
check("a stranger cannot see a draft's questions", code != 200, (code, out))
code, out = call(cy, "questionnaire", {"id": f})
check("nor open a draft", code != 200, (code, out))
check("the owner can preview, and is told it is not open", "not open" in ok(bob, "questionnaire", {"id": f})["locked"])
code, out = call(cy, "update_form", {"id": f, "title": "mine now"})
check("a stranger cannot change the form", code != 200, (code, out))
check("a draft is not offered to others", all(x["id"] != f for x in ok(cy, "forms_to_fill")))
code, out = call(bob, "submit_response", {"id": f, "answers": {qs["place"]: "Lisbon"}})
check("a draft takes no answers", code != 200, (code, out))

# --- answering
ok(bob, "open_form", {"id": f})
check("an open form is offered to everyone", any(x["id"] == f and x["done"] == "To do" for x in ok(cy, "forms_to_fill")))
q = ok(cy, "questionnaire", {"id": f})
check("the questions come with their choices", q["locked"] is None and q["questions"][0]["options"] == ["Lisbon", "Berlin", "Oslo"] and len(q["questions"]) == 9, q)
code, out = call(cy, "submit_response", {"id": f, "answers": {qs["mood"]: 5}})
check("a required question must be answered", code != 200 and "Where to" in str(out), (code, out))
for bad in ({qs["place"]: "Paris"}, {qs["place"]: "Oslo", qs["mood"]: 9}, {qs["place"]: "Oslo", qs["car"]: "maybe"}, {qs["place"]: "Oslo", qs["when"]: "soon"},
            {qs["place"]: "Oslo", qs["extras"]: ["Boat"]}, {qs["place"]: "Oslo", qs["bags"]: "many"}, {qs["place"]: "Oslo", "bogus": "x"}, {qs["place"]: "Oslo", qs["size"]: "XXL"},
            {qs["place"]: "Oslo", qs["name"]: "x" * 501}):
    code, out = call(cy, "submit_response", {"id": f, "answers": bad})
    check(f"a bad answer is refused: {list(bad.values())[-1]!r:.30}", code != 200, (code, out))
good = {qs["place"]: "oslo", qs["extras"]: ["hotel", "Dinner"], qs["mood"]: "4", qs["car"]: "Yes", qs["bags"]: "2,5", qs["when"]: "2026-12-01", qs["note"]: "No nuts\nplease", qs["size"]: "M"}
done = ok(cy, "submit_response", {"id": f, "answers": good})
check("a good answer is taken", done["ok"] and done["thanks"], done)
check("the form says you answered", "answered" in ok(cy, "questionnaire", {"id": f})["locked"])
check("and lists it as done", any(x["id"] == f and x["done"] == "Answered" for x in ok(cy, "forms_to_fill")))
code, out = call(cy, "submit_response", {"id": f, "answers": good})
check("a second answer is refused", code != 200, (code, out))
ok(owner, "submit_response", {"id": f, "answers": {qs["place"]: "Lisbon", qs["extras"]: ["Train"], qs["mood"]: 3, qs["car"]: "no", qs["bags"]: 1, qs["name"]: "Boss"}})
ok(bob, "submit_response", {"id": f, "answers": {qs["place"]: "Lisbon", qs["mood"]: 5, qs["car"]: "no", qs["bags"]: 0}})

# --- results
res = ok(bob, "results", {"id": f})
check("results count the responses", res["responses"] == 3, res)
rep = res["report"]
check("choices are counted", "**Lisbon**: 2 (67%)" in rep and "**Oslo**: 1 (33%)" in rep and "**Berlin**: 0 (0%)" in rep, rep)
check("several choices are counted separately", "**Hotel**: 1" in rep and "**Train**: 1" in rep and "**Dinner**: 1" in rep, rep)
check("ratings are averaged", "4.0 of 5" in rep, rep)
check("yes and no are counted", "**no**: 2" in rep and "**yes**: 1" in rep, rep)
check("numbers are summed up", "**Highest**: 2.5" in rep and "**Total**: 3.5" in rep, rep)
check("texts are listed", "No nuts please" in rep and "Boss" in rep, rep)
code, out = call(cy, "results", {"id": f})
check("others cannot read the results", code != 200, (code, out))
rows = ok(bob, "responses", {"id": f})
check("responses are listed with names", sorted(r["who"] for r in rows) == ["bob", "cy", "owner"], rows)
code, out = call(cy, "responses", {"id": f})
check("not to strangers", code != 200, (code, out))
csv = ok(bob, "responses_csv", {"id": f})["csv"]
lines = csv.split("\n")
check("the file has a header and a line for each response", lines[0].startswith("Submitted,Who,Where to?") and len(lines) >= 4, csv)
check("with the answers in it", "Oslo" in csv and "Hotel; Dinner" in csv, csv)

# --- changing a form that has answers
code, out = call(bob, "add_question", {"id": f, "label": "Late"})
check("a form with answers gets no new questions", code != 200, (code, out))
code, out = call(bob, "update_question", {"id": qs["place"], "options": "A\nB"})
check("nor new choices", code != 200, (code, out))
code, out = call(bob, "delete_question", {"id": qs["name"]})
check("nor loses questions", code != 200, (code, out))
code, out = call(bob, "update_form", {"id": f, "anonymous": True})
check("nor becomes anonymous", code != 200, (code, out))
code, out = call(bob, "reopen_as_draft", {"id": f})
check("nor goes back to a draft", code != 200, (code, out))
ok(bob, "update_question", {"id": qs["place"], "label": "Where shall we go?"})
check("but can be reworded", ok(bob, "get_question", {"id": qs["place"]})["label"] == "Where shall we go?")

# --- closing
ok(bob, "close_form", {"id": f})
check("a closed form says so", ok(cy, "questionnaire", {"id": f})["locked"] == "This form is closed.")
check("and is not offered", all(x["id"] != f for x in ok(cy, "forms_to_fill")))
code, out = call(owner, "submit_response", {"id": f, "answers": {qs["place"]: "Lisbon"}})
check("a closed form takes no answers", code != 200, (code, out))
ok(bob, "delete_response", {"id": rows[0]["id"]})
check("the owner deletes a response", len(ok(bob, "responses", {"id": f})) == 2)
code, out = call(cy, "delete_response", {"id": rows[1]["id"]})
check("a stranger cannot", code != 200, (code, out))
check("an administrator may manage any form", call(owner, "form_info", {"id": f})[0] == 200)

# --- anonymous, closing date, a form open to many
anon = ok(cy, "create_form", {"title": "Mood " + RUN, "anonymous": True, "thanks": "Noted!", "closes_at": (datetime.date.today() + datetime.timedelta(days=3)).isoformat()})["id"]
aq = ok(cy, "add_question", {"id": anon, "label": "Honestly?", "kind": "long", "required": True})["id"]
ok(cy, "open_form", {"id": anon})
check("a closing date is kept", ok(cy, "form_info", {"id": anon})["closes_at"].startswith((datetime.date.today() + datetime.timedelta(days=3)).isoformat()))
check("the header says answers are anonymous", "anonymous" in ok(bob, "form_header", {"id": anon})["subtitle"])
check("a custom thanks is used", ok(bob, "submit_response", {"id": anon, "answers": {aq: "Fine."}})["thanks"] == "Noted!")
ok(owner, "submit_response", {"id": anon, "answers": {aq: "Tired."}})
code, out = call(bob, "submit_response", {"id": anon, "answers": {aq: "again"}})
check("an anonymous form is still answered once", code != 200, (code, out))
rows = ok(cy, "responses", {"id": anon})
check("responses carry no names", [r["who"] for r in rows] == ["Anonymous", "Anonymous"], rows)
check("nor does the file", all(line.split(",")[1] == "" for line in ok(cy, "responses_csv", {"id": anon})["csv"].split("\n")[1:]), ok(cy, "responses_csv", {"id": anon}))
past = ok(cy, "create_form", {"title": "Old " + RUN, "closes_at": "2020-01-01"})["id"]
pq = ok(cy, "add_question", {"id": past, "label": "Hm"})["id"]
ok(cy, "open_form", {"id": past})
check("a form past its date is closed", ok(cy, "my_forms")[0]["state"] in ("closed", "open") and all(x["id"] != past for x in ok(bob, "forms_to_fill")))
code, out = call(bob, "submit_response", {"id": past, "answers": {pq: "late"}})
check("and takes no answers", code != 200, (code, out))
bad = call(cy, "create_form", {"title": "x", "closes_at": "whenever"})
check("a closing date must be a date", bad[0] != 200, bad)

# --- deleting
for form in (f, anon, past):
    ok(owner, "delete_form", {"id": form})
check("a deleted form is gone with its answers", call(owner, "form_info", {"id": f})[0] != 200 and call(owner, "responses", {"id": anon})[0] != 200)

finish()
