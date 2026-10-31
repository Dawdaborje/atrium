---
title: "Atrium roadmap"
description: "The order in which the Atrium modules are built, what each phase needs from the kernel, and which of the framework gap proposals it waits for."
status: stable
order: 201
---
# Atrium roadmap

**Status: all phases built.** Phases are ordered so that each one uses what already exists, and so that
work waiting on a framework proposal comes after it in the gaps roadmap.

## Phases

| Phase | Delivers | Needs |
|---|---|---|
| 0 Scaffold | Workspace `plugins/atrium`, this documentation | nothing |
| 1a Universal viewer (built, see page) | One document viewer for every file, with plugin-supplied viewers in JavaScript | Document viewer |
| 1 Drive and planner | `atrium_drive` (built: folders, files, sharing with folder inheritance, versions, trash, search by name and by the text inside files, guarded files from the first moment, links, comments, favourites, a storage quota, organization settings); `atrium_planner` (built: plans, buckets, tasks, checklists, labels, repeating tasks, reminders; see its `docs/design.md`) | viewer; media backend; History; OCR and `ai::search` |
| 2 People | `atrium_people` (built: directory, profiles, reporting lines, org chart; see its `docs/design.md`); `atrium_hr_link` (built: a channel for each department, kept in step with HR; see its `docs/design.md`) | [HRMS link](hrms_integration.md) |
| 3 Calendar | `atrium_calendar` (built: personal and shared calendars, events that repeat, invitations and answers, clash warnings, agenda, availability; see its `docs/design.md`); `atrium_booking` (built: services with weekdays, hours and buffers, free times from the host's calendar, booking and cancelling, in the host's time zone and in the guest's; outsiders book through a public page at `/book/<link>/<service>`; see its `docs/design.md`). The calendar knows each person's time zone, and a booking link (`cal_link`) lets outsiders see free times without an account | recurrence rules, ICS, week/day/agenda views (kernel) |
| 4 Chat | `atrium_chat` (teams, channels, groups, direct messages, threads, files; first version built, see its `docs/design.md`; reactions, mentions and search to come) | two-way channel and presence (kernel, built) |
| 5 Knowledge and sites | `atrium_wiki` (built: spaces with roles, page tree, `[[links]]`, history and restore, search); `atrium_news` (built: authors, drafts, pinning, likes, read counts, comments); `atrium_forms` (built: nine question kinds, results, CSV, the `<questionnaire>` page widget); `atrium_sites` (built: team start pages with quick links and the latest news); list templates not built | Markdown with app links (kernel, built); a rich-text field with sanitizer would replace Markdown editing |
| 6 Mail | `atrium_mail` (built: shared inboxes, plus-tag boxes, conversations, take/assign/close, replies that come back into the conversation; see its `docs/design.md`); `atrium_mailbox` (built: a person's own mailbox on a JMAP or an IMAP server, read, answer, write, with attachments both ways; see its `docs/design.md`) | Inbound mail (built); the `jmap` and `imap` bridges and per-person logins (built, see Bridges) |
| 7 Meetings and office files | `atrium_meet` (built: meet now, plan, invite, join with a link made for one person and one room; the video runs on a Jitsi Meet server); editing office files online (built in `atrium_drive` 0.3: "Edit online" for Word, Excel, PowerPoint and Open Document files, each save a version) | the `jitsi` bridge; the `collabora` bridge and the kernel's WOPI endpoints (built, see Bridges). The video and the editor are servers the organization runs; Aether does not bundle them |

Planner rules and approvals wait for Automation; Gantt waits for Views and reports;
link sharing and per-item permissions wait for Sharing; custom library columns wait for
Customization. Until then each module ships a plainer fallback and names it in its design page.

## Plugin layout

One core plugin per area holds the masters, satellites add features: `atrium_drive`, `atrium_planner`, ... The
workspace follows the manifest of the other workspaces (`workspace.toml`, one folder per plugin with `plugin.toml`,
`models/`, `rules/`, `pages/`, `docs/design.md`). Every plugin ships its own documentation through `[docs]` in
`plugin.toml`, so the assistant and the docs site see it. Each plugin gets an end-to-end script, as in the helpdesk workspace.

## Per-phase definition of done

- Models, rules and pages load with `aether --load-plugins --dry-run`.
- An end-to-end script passes against a throwaway instance.
- `docs/design.md` and the plugin documentation exist; status lines on this page are updated.
- Events the module announces are listed, so other plugins (and webhooks) can rely on them.
