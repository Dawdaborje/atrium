---
title: "Atrium and HRMS"
description: "How the Atrium workplace modules connect to the HR plugins through events and plugin calls, without either side depending on the other."
status: stable
order: 202
---
# Atrium and HRMS

**Status: partly built.** Built: `atrium_hr_link` copies job, department, work contact details, location and manager onto each person's directory profile (`atrium_people`, locked for editing there), and makes a chat channel for each department (team Departments) and keeps its members in step with HR (the sync command and a nightly run catch transfers; `employee_status_changed` and `employee_hired` events bring a channel up to date at once and take people who left out of every shared channel). Not built: the other rows of the table below (the people-directory row is built, except that photos are not copied). Atrium must run without the HR plugins, and HR must run without Atrium. The
connection is one small plugin, `atrium_hr_link`, that depends on `hr` and on the Atrium plugins. Core Atrium
plugins never list `hr` in their dependencies.

## Mechanism

The same pair of tools the HR plugins already use between themselves (Events):

- **Events**: `atrium_hr_link` listens to events the HR plugins announce and calls Atrium plugin functions in response.
- **Plugin calls**: it reads people with `plugins::call("hr", "get_employee", ...)`. A field it manages is written
  only by itself (`"write_roles": ["via:self"]`, see Rules).
- **Fallback**: without HR, `atrium_people` works from the login accounts and what people enter themselves. The link pushes HR's data into it (`apply_hr`), so the directory never depends on HR.

## Connection points

| HR side | Atrium side |
|---|---|
| `employee` (photo, display name, manager, department, work email and phone, status) | Profile fields in `atrium_people` (title, department, work phone and email, location, manager), which feed the directory and the org chart. `employee_private` is never read. |
| `department` | A team (chat), a team site and a drive folder per department; membership follows the employee's department. |
| `leave_approved`, `employee_status_changed` (on leave) | An all-day "out of office" item in the calendar; presence in chat. |
| `onboarding_run_started` | A plan in the planner: one bucket per onboarding step, one task per onboarding task. |
| `onboarding_task_done`, `onboarding_task_overdue` | The matching planner task is completed or flagged overdue; and the reverse for tasks completed in the planner. |
| `employee_hired` | Chat account, drive home folder, membership of the department team. |
| `employee_departed` | Home folder archived and ownership handed to the manager; memberships removed. |
| `hr_documents` | Employee documents stored as drive files, with a link back to the record. |
| Training sessions, interviews (`hr_training`, `hr_recruitment`) | Calendar events with the attendees. |

## Rules

- Link plugins write through the public functions of the target plugin, never straight into its tables.
- Every change made by a link is audited as the acting user or a named service role, like any plugin call.
- Each connection is a setting that an administrator can switch off.
- The reverse direction (HR reading Atrium) is limited to links shown on the employee page, for example "open the
  employee's drive folder", gated by the viewer's own access.
