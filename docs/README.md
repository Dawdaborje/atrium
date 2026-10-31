---
title: "Atrium: an open-source workplace suite"
description: "Which parts of a workplace suite Aether can rebuild (files, planning, calendar, chat, knowledge, intranet sites, mail), which cannot, and how the pieces fit together."
status: stable
order: 200
---
# Atrium: an open-source workplace suite

**Status: built (see the [roadmap](roadmap.md)).** Atrium is the workspace `plugins/atrium/` (plugins named `atrium_*`). It is a
suite of ordinary Aether plugins on the existing kernel; it adds no private back door. Where the kernel is missing
something, the gap is listed in [Platform gaps](platform_gaps.md) and built as a general kernel feature.

Pages: [Roadmap](roadmap.md) · [Platform gaps](platform_gaps.md) · [Connecting to HRMS](hrms_integration.md) ·
Universal document viewer.

## What can be rebuilt

| Product area | Atrium module | Verdict |
|---|---|---|
| Team chat | `atrium_chat` | Yes. Good behaviour (typing, presence) needs a two-way channel in the kernel. |
| Video meetings | `atrium_meet` | Built as link-out: the `jitsi` bridge issues a join link to a Jitsi Meet server. |
| Task planning | `atrium_planner` | Yes, now. Boards exist; rules need Automation, Gantt needs Views and reports. |
| File storage and libraries | `atrium_drive` | Yes, now. Versions come from History; link sharing needs Sharing. |
| Co-editing office files | `atrium_drive` | Through an external editor: the `collabora` bridge and the kernel as a WOPI host. Aether has no editor of its own. |
| Calendar | `atrium_calendar` | Yes, with recurrence, invitations and week/day views added to the kernel. |
| Bookable time slots | `atrium_booking` | Yes, on the calendar. |
| Contacts | `party` | Already there; `atrium_people` adds the directory. |
| Mail | `atrium_mail`, `atrium_mailbox` | Shared inboxes on Inbound mail; a personal mailbox through the `jmap` bridge to a real mail server. |
| Intranet sites, pages, lists | `atrium_sites` | Yes, mostly by composing the other modules (see below). |
| Notes and wiki | `atrium_wiki` | Yes, after a rich-text field type. |
| Forms and surveys | `atrium_forms` | Yes: a form definition model plus public pages. |
| Company news | `atrium_news` | Yes: posts with chatter comments. |
| Flow automation, device management, security suites, directory | none | Out of scope; automation is a kernel feature, sign-in is covered by bridges. |

## Intranet sites, libraries and lists

A site groups what a team uses: a document library, a wiki space, a planner, a news feed and optional lists.

- **Sites**: `site`, `site_member`, `site_nav` and a home page. A team site can follow a chat team or an HR department.
- **Document libraries**: a library is a root folder of `atrium_drive` with its own metadata columns, approval and
  retention. Versions use history, the audit chain gives the compliance record. Custom columns need
  Customization; until then a library keeps a JSON `metadata` field.
- **Lists**: already native. User-defined models with list, form, kanban and calendar views are lists with views.
  Atrium ships a gallery of list templates (issue tracker, contacts, assets) as seed models.
- **Pages and news**: wiki pages with a layout built from the existing board and dashboard widgets; news posts with
  comments; public communication sites through visitor pages.
- **Permissions**: site roles (owner, member, visitor) map to roles and access rules. Per-item and link sharing wait
  for Sharing. Broken permission inheritance is deliberately not copied.
- **Search**: `ai::index` and `ai::search` over page and file text (OCR included), filtered per site.
- **Not replicated**: server farm administration, form and app builders from other suites, office web parts and a
  desktop sync client (clients use API tokens and the record API).

## Principles

1. **Optional links, no hard dependencies.** Atrium works without the HR plugins; the link is one small plugin.
2. **Reuse before building.** Files go through the media backend, comments through chatter, history for versions,
   events and webhooks for integration, `party` for people.
3. **Gaps become kernel features**, designed once and documented under Architecture, never as a
   private extension of one plugin.
