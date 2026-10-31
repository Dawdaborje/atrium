# Atrium

Open-source workplace suite for Aether: the files, planning, calendar, chat, knowledge and intranet tools a team
uses every day. Not every product of the big suites can be rebuilt; the docs say which can and in what order.

`atrium` is the home page and the entry on the Apps launcher; it links to every module below.

Built (each has `docs/design.md` and an end-to-end script in `tests/`): `atrium_drive` (with online editing of office
files), `atrium_planner`, `atrium_people`, `atrium_hr_link`, `atrium_calendar`, `atrium_booking`, `atrium_chat`, `atrium_wiki`,
`atrium_news`, `atrium_forms`, `atrium_sites`, `atrium_mail` (shared inboxes), `atrium_mailbox` (a person's own mail), `atrium_meet`.

Three of them use servers the organization runs: a Jitsi Meet server (`atrium_meet`), an office editor such as Collabora Online
(`atrium_drive`) and a JMAP or IMAP mail server (`atrium_mailbox`). An administrator switches on the matching bridge and fills in its settings; see
`aether/docs/bridges.md`. Outsiders can book time on a public page (`atrium_booking`, `atrium_calendar` booking links) and mailboxes carry attachments. A personal mailbox works on a JMAP server or on a plain IMAP server (with SMTP for sending).

Design and roadmap live in this workspace: `docs/`. Setup for the tests: `tests/README.md`.
