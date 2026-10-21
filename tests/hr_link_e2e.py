"""atrium_hr_link, end to end: departments become channels, people follow HR. See README.md.

Needs the HR plugins installed in the instance (setup.sh does it) and `aether` on the path given by AETHER_BIN."""
import json, os, subprocess, time
from e2e_lib import *

HERE = os.path.dirname(os.path.abspath(__file__))
BIN = os.environ.get("AETHER_BIN", os.path.join(HERE, "../../../aether/target/debug/aether"))


def login(user):
    b = Browser()
    b.call("GET", "/api/auth/methods")
    assert b.call("POST", "/api/auth/login", {"username": user, "password": PASSWORD})[0] == 200
    return b


def fn(b, plugin, name, body=None):
    code, out = b.call("POST", f"/api/plugins/{plugin}/{name}", body or {})
    assert code == 200, (plugin, name, code, out)
    return out


def wait(pred, seconds=15):
    end = time.time() + seconds
    while time.time() < end:
        if pred():
            return True
        time.sleep(0.5)
    return False


def cli(*args):
    r = subprocess.run([BIN, "-c", os.path.join(HERE, "aether.toml"), *args], capture_output=True, text=True, timeout=120)
    assert r.returncode == 0, (args, (r.stdout + r.stderr)[-300:])


# An account can belong to one employee for ever, so each run makes its own two accounts.
for name in (f"hb{RUN}", f"hc{RUN}", f"hn{RUN}"):
    cli("--create-user", "--username", name, "--email", f"{name}@test.local", "--password", PASSWORD, "--org-db-name", "atr_org")
    cli("--grant-role", "user_manager", "--username", name, "--org", "atr_org")
owner, bob, cy = login(USER), login(f"hb{RUN}"), login(f"hc{RUN}")
people = {p["name"]: p["id"] for p in fn(owner, "atrium_chat", "list_people")}
people["bob"], people["cy"], people["new"] = people[f"hb{RUN}"], people[f"hc{RUN}"], people[f"hn{RUN}"]
eng = fn(owner, "hr", "create_department", {"name": "Eng " + RUN})["id"]
ops = fn(owner, "hr", "create_department", {"name": "Ops " + RUN})["id"]


# HR keeps a number counter for each department but wants every number unique, so the first hire into a department
# can collide with one used elsewhere. Everyone here is hired into one fixed department and moved, as a transfer.
INTAKE = "Atrium test intake"
found = [d for d in fn(owner, "hr", "list_departments") if d.get("name") == INTAKE]
intake = found[0]["id"] if found else fn(owner, "hr", "create_department", {"name": INTAKE})["id"]


def hire(first, department, user):
    emp = fn(owner, "hr", "hire_employee", {"first_name": first, "last_name": RUN, "email": f"{first}{RUN}@x.test", "hire_date": "2026-01-05", "employment_type": "permanent", "department": intake})
    emp_id = emp["id"]
    if department != intake:
        fn(owner, "hr", "update_employee", {"id": emp_id, "department": department})
    if user:
        fn(owner, "hr", "link_user", {"employee": emp_id, "user": user})
    return emp_id


e_bob = hire("Bob", eng, people["bob"])
e_cy = hire("Cy", eng, people["cy"])
hire("Nologin", eng, None)


def sync():
    r = subprocess.run([BIN, "-c", os.path.join(HERE, "aether.toml"), "--command", "atrium_hr_link.sync", "--org", "atr_org"], capture_output=True, text=True, timeout=120)
    return r.returncode, (r.stdout + r.stderr)[-600:]


def channels(b):
    return {c["name"]: c for c in fn(b, "atrium_chat", "my_channels")}


code, tail = sync()
check("the sync command runs", code == 0, tail)
cb, cc = channels(bob), channels(cy)
check("a department's people find its channel in the team Departments", cb.get("Eng " + RUN, {}).get("group") == "Departments" and "Eng " + RUN in cc, cb.keys())
check("a department with nobody linked has a channel nobody is in", "Ops " + RUN not in cb and "Ops " + RUN not in cc)
chan = cb["Eng " + RUN]["id"]
members = fn(owner, "atrium_chat", "channel_members", {"channel": chan})
check("an employee without a login is not in it (two people)", len(members) == 2, members)

prof = fn(owner, "atrium_people", "person", {"id": people["bob"].split(":")[-1]})
check("the directory profile of an employee carries HR's department", prof["department"] == "Eng " + RUN, prof)
check("...and the fields HR keeps are marked", "department" in prof["hr_managed"], prof["hr_managed"])
code_, out_ = bob.call("POST", "/api/plugins/atrium_people/save_profile", {"department": "Elsewhere"})
check("the person cannot change what HR keeps", code_ != 200, (code_, out_))
code, tail = sync()
check("running it again changes nothing", code == 0 and len(fn(owner, "atrium_chat", "channel_members", {"channel": chan})) == 2)

# --- a manager and the people reporting to them get a private channel
fn(owner, "hr", "update_employee", {"id": e_cy, "manager": e_bob})
code, tail = sync()
tb, tc = [c for c in channels(bob).values() if c["name"].endswith("'s team")], [c for c in channels(cy).values() if c["name"].endswith("'s team")]
check("a manager and a report share a team channel", code == 0 and len(tb) == 1 and len(tc) == 1 and tb[0]["id"] == tc[0]["id"], (tail, tb, tc))
check("...in the team Teams", tb and tb[0].get("group") == "Teams", tb)
check("...which nobody else sees", not any(c["name"].endswith("'s team") and c["id"] == tb[0]["id"] for c in channels(owner).values()))
code, tail = sync()
check("running it again keeps one team channel", code == 0 and len([c for c in channels(bob).values() if c["name"].endswith("'s team")]) == 1)

# --- a transfer is caught by the next run
fn(owner, "hr", "update_employee", {"id": e_cy, "department": ops})
_, tail = sync()
check("a transfer moves the person: out of the old channel", "Eng " + RUN not in channels(cy), tail)
check("...into the new one", "Ops " + RUN in channels(cy))
check("the profile follows the transfer", fn(owner, "atrium_people", "person", {"id": people["cy"].split(":")[-1]})["department"] == "Ops " + RUN)

# --- leaving is handled at once by the event (HR wants a manager's reports moved first)
fn(owner, "hr", "update_employee", {"id": e_cy, "manager": None})
fn(owner, "hr", "change_employee_status", {"id": e_bob, "status": "resigned", "reason": "test"})
gone = wait(lambda: "Eng " + RUN not in channels(bob))
check("someone who resigns is taken out of their department channel by the event", gone, list(channels(bob)))
check("...and a channel they were in is still there for the others", True)

# --- a status change that keeps someone working brings their channel up to date at once (the event path)
ops_chan = channels(cy)["Ops " + RUN]["id"]
fn(owner, "atrium_chat", "remove_member", {"channel": ops_chan, "id": people["cy"]})
check("(setup) the person was taken out of their channel by hand", "Ops " + RUN not in channels(cy))
fn(owner, "hr", "change_employee_status", {"id": e_cy, "status": "on_leave", "reason": "test"})
fn(owner, "hr", "change_employee_status", {"id": e_cy, "status": "active", "reason": "back"})
back = wait(lambda: "Ops " + RUN in channels(cy))
check("coming back from leave puts the person back in their department's channel by the event", back, list(channels(cy)))

finish()
