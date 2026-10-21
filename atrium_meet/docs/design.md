# atrium_meet: design

Video meetings, written in Rust. The video itself runs on a Jitsi Meet server; this plugin plans meetings and gives each person a link
that opens the right room.

## The server

An administrator switches on the bridge `jitsi` (Settings, bridges) and sets `bridge.jitsi.base_url`. A public server (the default,
meet.jit.si) needs nothing else. For the organization's own server, set `bridge.jitsi.app_id` and `bridge.jitsi.signing_key` to the server's
token authentication: every link is then signed (HS256) for **one room, one person and four hours**, and says whether that person moderates.
A link taken from a chat cannot open another room, and it stops working. Without the token setup a room is open to anyone who knows its address,
which is why the room names are made of the meeting's random key (`aether-<key>`) and are never shown to people outside the meeting.

## Models

| Model | Holds |
|---|---|
| `meet_meeting` | `title`, `host`, `room`, `starts_at` (empty for "now"), `minutes`, `about`, `event` (the calendar event made for it) |
| `meet_guest` | `meeting`, `user`: who is invited |

Only the plugin's own functions read and write them; the host and the guests see a meeting, nobody else ("there is no such meeting").

## What people do

- **Meet now** (`start_now`): a meeting without a start time, the guests are told, and the host's link is returned at once.
- **Plan** (`create_meeting`): title, start (`2026-11-02T14:00`, in the host's own time zone from the calendar's Time zone page; UTC without one), length (5 minutes to 8 hours), guests, a note. Guests get a notification.
- **Join** (`join_meeting`): asks the bridge for a link for this person (the host moderates) and answers `{ open_url }`; the page opens it in a new tab.
- **Cancel** (`cancel_meeting`): the host only; the guests are told.

The list shows meetings of the last day and everything ahead. A planned meeting also enters the calendar (`atrium_calendar`): an event in the host's
calendar with the guests invited, whose location is the meeting's page (`/meet/m/<id>`). Cancelling the meeting takes the event out. A guest who
answers "no" in the calendar is still on the meeting's guest list. Changing a meeting is cancel and plan again.
The list shows the start in the reader's zone.
