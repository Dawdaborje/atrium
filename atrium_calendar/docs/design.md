# atrium_calendar: design

Calendars, events, invitations and availability, written in Rust. Everyone has a calendar of their own; shared calendars
have owners, editors and viewers. An event's conversation (comments, followers, the history of what changed) is the
kernel's chatter.

## Models

| Model | Holds |
|---|---|
| `cal_calendar` | `name`, `description`, `color`, `owner`, `personal` |
| `cal_member` | `calendar`, `user`, `role` (`owner`, `editor` or `viewer`) |
| `cal_event` | `calendar`, `title`, `description`, `location`, `starts_at`, `ends_at`, `all_day`, `repeat`, `until`, `attendees`, `attendee_keys`, `attendee_names`, `created_by` |
| `cal_response` | `event`, `user`, `status` (`needs_action`, `accepted`, `tentative`, `declined`) |
| `cal_skip` | `event`, `date`: a day of a repeating event that does not happen |

Rules let only the plugin's own functions touch these (`via:self`). The exception is read access to `cal_event` for people
whose calendar list holds the event's calendar (`"calendar": "$my_calendars"`, answered by `rule_var_my_calendars`), which
is what lets chatter show an event's conversation to the calendar's members.

## Access

A person who is not in a calendar cannot see it or its events ("there is no such calendar"). Viewers look, editors add,
change and delete events, owners also rename the calendar, delete it and manage people. A calendar keeps one owner; anyone
may leave a shared calendar. A personal calendar (made the first time someone opens the plugin) cannot be deleted or left.
People who are invited to an event see and open it without being in its calendar, can answer it, and cannot change it.

## Repeating events

`repeat` is `none`, `daily`, `weekly`, `monthly` or `yearly`, optionally until a date. The month grid asks for a window and
gets the occurrences inside it, each with the event's id. Monthly and yearly repeats keep the day of the first occurrence
(31 Jan, 28 Feb, 31 Mar). A single day can be left out (`skip` in the form, or `delete_occurrence`).

## Invitations and clashes

Inviting notifies. Moving an event clears the answers and notifies the invited. An answer notifies the person who made the
event. When people are invited, the reply lists those who are busy at that time; their events are never shown, only that
they clash. Busy means: their own calendar's events, and events they are invited to and have not declined. All-day events
do not make a person busy.

## Availability and reminders

`/calendar/availability` shows each person as free or busy now, until when, and when they are next busy this week, never
what the events are. `morning_agenda` (07:00) tells everyone with events today; `starting_soon` (every five minutes)
tells the invited of events that start in ten to fifteen minutes.

## Files

`export_ics` writes a calendar as iCalendar text (repeat rules, left-out days, whole-day events); `import_ics` reads events
from it. A rule other than a plain daily, weekly, monthly or yearly repeat (every second week, certain weekdays, a count)
is imported as a single event and counted in `rules_simplified`. The page is `/calendar/c/{calendar}/ics`.

## For the booking plugin

`busy_of` (a person's busy intervals between two moments, at most two months, never titles), `book_slot` (a meeting in a
host's own calendar with the caller invited and accepted; refused when the host is busy) and `cancel_booked` (the person
who booked it or the calendar's owner removes it). See `atrium_booking`.

## Events announced

`event_created`.

## Time zones

Everything is stored in UTC. Each person can choose a zone on **Time zone** (`/calendar/timezone`, `save_timezone`): they then type times in it and read
them in it (the month grid, the agenda, the event form, the notes people get), and the list of who is free uses it too. A time written with a `Z` or an
offset is taken as it says. Without a setting a person is read as UTC, as before zones existed (times leave with a `Z` and the browser shows them in its own
zone). Whole-day events are dates and never move.

An event remembers the zone it was made in (`tz`). A repeating event keeps its wall-clock time there through the clock changes (nine o'clock stays nine o'clock),
so its occurrences are laid out in that zone and turned back into UTC; the days left out are dates of that zone. In `.ics` files a time with a `TZID` is
applied when read, a time without one is read in the importer's zone, and files go out in UTC. For other plugins: `typed_to_utc` and `read_times`.

## Booking links

For people outside the organization, a member has one booking link (`make_link`, `revoke_link`; the `cal_link` record's key is the secret in the address). Three
functions answer anyone who holds the token, for that member only: `link_busy` (busy intervals, never titles), `link_book` (a meeting in the member's
own calendar, with the guest's name and address in its notes; refused when the member is busy, outside 120 days ahead, or when the guest or the member
has too many waiting) and `link_cancel` (only a meeting a guest booked). They are the only `public_functions` besides `rule_var_my_calendars`, which
the record rules need and which answers nothing to a visitor. The booking plugin builds its public pages on them.

## Known limits

The grid is a month; week, day and agenda grids wait for kernel views (the plain agenda list exists). No rooms or resources, no subscribing to calendars from other servers, no per-event privacy. An event cannot be shown in another zone than the reader's own, and
the zone of an event is not changed afterwards.

## Tests

`tests/calendar_e2e.py` (HTTP). Unit tests (`cargo test`) cover repeats, moments, files and the pages' text.
