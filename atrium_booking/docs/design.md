# atrium_booking: design

People offer time, colleagues book it. Booking reads free time from `atrium_calendar` and writes the meeting there; it
never sees what a busy time is.

## Models

| Model | Holds |
|---|---|
| `bk_service` | `owner`, `owner_name`, `title`, `description`, `location`, `minutes`, `buffer`, `notice_hours`, `days_ahead`, `weekdays` (1 Monday to 7 Sunday), `from_time`, `to_time`, `tz` (the zone of the hours), `active`, `public`, `link` |
| `bk_booking` | `service`, `title`, `host`, `booker`, `starts_at`, `ends_at`, `status` (`confirmed` or `cancelled`), `event` (the calendar event), `note`; for a guest outside the organization `booker` is empty and `guest_name`, `guest_email` and `link` are set |

Both are closed by their rules (`via:self`); pages and people reach them only through the plugin's functions.

## Free times

A day is cut into whole meetings from `from_time`; a time is offered when it is on one of the chosen weekdays, no sooner than
`notice_hours` from now, within `days_ahead` days (at most 60), and when the host is not busy in it or within `buffer` minutes
of it. Busy comes from `atrium_calendar.busy_of`: only intervals, never titles; all-day events do not count, and events
the host was invited to and declined do not either.

**Time zones.** The days and hours of a service are wall-clock times in its own zone (`tz`, Europe/Oslo and so on; the host's calendar zone when they have one, else UTC), so 09:00 stays 09:00 through the clock changes. Free times are kept in UTC and shown to a colleague in their own calendar zone (`atrium_calendar.read_times`); a visitor from outside sees them in the service's zone, which the page says.

## Booking

`book` takes a row of the free-times list (`service|start`). The slot is recomputed on the server, so a time that was taken in
the meantime is refused ("that time is not free any more"). The meeting is created by `atrium_calendar.book_slot` in the
host's own calendar with the guest invited and already accepted, the host is notified, and a `bk_booking` row ties the two
together. Nobody can book their own time. Either side cancels with `cancel_booking`: the calendar event goes, the booking
becomes `cancelled`, and the other person is told. A service with upcoming bookings cannot be deleted, only switched off.

## Pages

`/booking` (find a meeting and its host), `/booking/s/{id}` (free times, a Book button on each), `/booking/mine`,
`/booking/services` and `/booking/service/{id}` (what I offer).

## Public booking (people outside the organization)

A host switches **Open to people outside** on in a service. The service then has a public address `/book/<link>/<service>`: `<link>` is the host's
booking link in the calendar (`atrium_calendar.make_link`, one for each member, whose key is the secret in the address), so the address can be
shared and stopped (switch the service off, or `revoke_link`, and it answers no more). The pages `/book/<link>/<service>` (pick a free time, give a name
and an email address, add a note) and `/book/done/<token>` (the booking, with a Cancel button) are public pages; the five public functions
(`public_service`, `public_slots`, `public_book`, `public_booking`, `public_cancel`) check the link or the booking token on every call.

A visitor reaches the host's calendar only through the calendar's three link functions (`link_busy`, `link_book`, `link_cancel`): busy intervals
without titles, a meeting put in the host's calendar (the guest's name and address in its notes), and taking back a meeting a guest booked. A guest
holds at most three meetings at once on one offer and a host at most 150 waiting guest meetings; a time must be inside the offer's days ahead. The
token of the booking, in the address of the second page, is the only way for the guest to cancel; no mail is sent (the organization's mail bridge
would be a natural way to send it). The server must tell visitors which organization they mean: `[tenancy] org_resolution` (the page "Visitors, public pages and the audit trail" in the platform documentation).

## Known limits

Hosts are members of the organization; guests cannot see or change anything but their own booking. No group sessions
(one guest per time), no approval step, no reminders of its own (the calendar's apply, since the guest is invited).

## Tests

`tests/booking_e2e.py` (HTTP). Unit tests cover the cutting of a day, notice, and breaks.
