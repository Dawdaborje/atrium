# atrium_hr_link: design

The only place that knows both HR and Atrium (chat and the people directory). HR is the record of who works in which department; chat is where
they talk. This plugin reads the first through HR's public functions and writes the second through atrium_chat's
service functions, so neither plugin depends on the other and either can be installed alone.

## What it does

| HR says | Chat gets |
|---|---|
| A department exists (`hr.list_departments`, active ones) | A channel with its name in the team **Departments**, made by `atrium_chat.ensure_channel` |
| People work in it (`hr.list_employees`, with a linked login in `employee.user`, status not resigned, terminated, retired or deceased) | They are its members (`atrium_chat.sync_members`); people who moved out are taken out, the owner stays |
| `hr.employee_hired`, `hr.employee_rehired`, `hr.employee_status_changed` | That department's channel is brought up to date at once; a person whose status means they left is removed from every shared channel (`atrium_chat.remove_everywhere`); direct messages and private conversations are not touched |
| An employee with a login: job (else position), department, work email and phone, work location text, manager's login | The person's directory profile (`atrium_people.apply_hr`); those fields are then marked as HR's and cannot be edited in the directory. A manager line that would loop is left out |
| An employee with a login who has reports with logins | A private channel "<name>'s team" in the team **Teams** with the manager and those reports (`ensure_channel` with `private`); it follows changes in reporting lines and people who left. When a manager has no reports left the channel stays, with its history |
| Approved leave (`hr_leave.leave_approved`) | The person is shown as away in the directory until the leave ends (`atrium_people.set_away`); the nightly run clears what has ended |
| (nothing: HR announces no transfer) | The nightly run `departments-nightly` (00:40) catches transfers, and also brings every profile up to date |

Run it by hand with `aether --command atrium_hr_link.sync --org <org>`.

## Rules

- An employee with no linked login (`employee.user`) has no chat account and is skipped.
- Chat is never the record of anything: removing someone here changes only their membership.
- The service functions of atrium_chat accept the kernel's own work and administrators; an event handler and a scheduled
  job run as the kernel, so this plugin needs no role.

## Not built yet

A channel for each project, and onboarding steps that post into a new hire's welcome channel.
