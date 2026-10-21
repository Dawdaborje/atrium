# atrium_people: design

A directory of the people in the organization, with profiles and an org chart, written in Rust. Everyone who can sign in
is listed (`org::members`); a profile adds what a person says about themselves.

## Model

`people_profile`: `user` (the login id), `title`, `department`, `manager` (a login id), `email`, `phone`, `mobile`,
`location`, `skills` (comma separated), `bio`, `photo`, `hr_managed` (the fields the HR link keeps) and `updated_at`.
A person with no profile is still listed, with empty fields. Rules let only the plugin's own functions touch it (`via:self`).

## Who may do what

Everyone in the organization may read every profile; the fields are meant to be shared (no pay, no private details). A
person edits their own profile; an organization administrator edits anyone's. A field HR keeps cannot be changed by the
person: the form is sent back whole, so an unchanged value is accepted and a changed one is refused with the reason.

## Reporting lines and the chart

`manager` is another member. Nobody reports to themselves, and a change that would make a loop (a manager who already
reports, through others, to this person) is refused. The org chart starts from everyone who reports to nobody (or to
someone no longer a member) and lists each manager before their reports, indented by depth, up to 12 levels and 2,000
people. `org_chart` takes an `id` to draw one person and everyone below them.

## With the HR link

Without any other plugin the directory works from what people enter. With `atrium_hr_link` installed, job title,
department, work contact details, location and manager come from the HR record (`apply_hr`, for the kernel's own jobs and
administrators), the nightly sync and the employee events keep them current, and those fields are locked for the person.
`release_hr` gives the fields back.

## Functions

Looking: `directory` (`q`, `department`), `department_options`, `person`, `my_profile`, `person_reports`, `person_chain`,
`org_chart`, `pick_people`. Changing: `save_profile`. For the link: `apply_hr`, `release_hr`. A profile change emits
`profile_changed`.

## Pages

`/people` (search the directory), `/people/{id}` (a profile, its direct reports and the line above), `/people/me` and
`/people/{id}/edit` (the form), `/people/chart/{id?}`.

## Also in the pages

The directory shows photos and has a department filter; `/people/upcoming` lists birthdays and work anniversaries; a person can
say they are away (`away_until`, `away_note`), shown next to their name; `/people/chart` draws the org chart.

## Not built

"Out of office" taken from approved leave (see the HR link).

## Tests

`tests/people_e2e.py` (HTTP) and `tests/people_ui_e2e.mjs` (Chromium); `tests/hr_link_e2e.py` covers the HR fields.
