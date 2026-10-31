---
title: "Atrium: platform gaps"
description: "What the kernel already provides for the Atrium modules, what waits on the framework gap proposals, and what has to be built for Atrium."
status: stable
order: 203
---
# Atrium: platform gaps

**Status: what Atrium needed and the kernel lacked has mostly been built (the questionnaire page widget, app links in Markdown, `redirect` that uses the function's answer, inbound mail and its CSRF exemption). The bridges for meetings (`jitsi`), office editing (`collabora`, with the kernel as a WOPI host) and personal mailboxes (`jmap`, with per-person logins) are built too.**

## Built and ready to use

| Need | Provided by |
|---|---|
| Versions, undo and restore of files, pages and tasks | History (`"history": true` on a model) |
| Tamper-evident record of who did what | Audit chain |
| Clients, scripts, a future sync tool | API tokens and the record API |
| Connectors, chat bots, notifications to other systems | Outgoing webhooks, same page |
| Search across files and pages | `ai::index`, `ai::search` (AI) with OCR text |
| An assistant that works over Atrium data | The assistant uses plugin functions as tools (AI) |
| File storage (local or S3) | Media |
| Comments, mentions, followers | Chatter |
| Live updates and notifications | Server-sent events (Notifications) |
| Reminders, recurrence of jobs | Scheduler |
| People and organizations | Party |
| Kanban, list, form, board, month calendar | Built-in views |

## Waiting on a framework proposal

| Need | Proposal | Atrium module affected |
|---|---|---|
| Share a file or record by link, with expiry | Sharing | drive, sites |
| Rules, assignment, approvals, repeat | Automation | planner, drive libraries |
| Gantt, pivot, tree views | Views and reports | planner |
| Email into records (built) | Inbound mail | mail |
| Per-organization fields and layouts | Customization | library columns, lists |
| Import and export | Import and export | drive, planner |
| Print and PDF | Print | wiki, forms |

## Not on any roadmap yet: built for Atrium as general features

| Gap | Used by | Phase |
|---|---|---|
| Universal document viewer with plugin viewers (page) | drive and every file field | 1a |
| Folder tree, thumbnails, per-file version listing in the UI | drive | 1 |
| Recurrence rules, ICS import/export, week/day/agenda views, attendees and free/busy | calendar, booking | 3 |
| Two-way channel (WebSocket), presence and typing: Realtime; per-channel unread counters live in the plugin | chat | 4 |
| Panes, message list, composer, rail, plugin layouts: Custom pages | chat, mail | 4 |
| Message attachments in chatter (planned in Chatter) | chat, news | 4 |
| Rich-text field type with an HTML sanitizer | wiki, news, sites | 5 |
| Full-text search across models (beyond `ai::search`) | sites | 5 |
| Mail server bridges: JMAP (`jmap`) and IMAP with SMTP sending (`imap`), both built | mailbox | 6 |
| Media-server bridge (token and join link): built (`jitsi`) | meet | 7 |
| Office-editor bridge (WOPI style): built (`collabora`) | drive | 7 |
