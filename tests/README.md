# Atrium end-to-end checks

`chat_e2e.py` drives `atrium_chat` over HTTP and a real WebSocket: teams and channels, public and private access,
direct messages and groups, threads, editing and deleting, the live topics and their access rules.

`hr_link_e2e.py` drives `atrium_hr_link` against the HR plugins: departments become channels, people follow transfers, leavers are taken out, a status change updates a channel at once. HR keeps an employee-number counter for each department but wants every number unique, so the test hires everyone into one fixed department and moves them.

`drive_e2e.py` drives `atrium_drive` over HTTP (76 checks), above all that a guarded file's bytes are served only to the people the drive allows: sharing, folder inheritance, versions, trash, deleting for good. `drive_more_e2e.py` covers the rest of the drive: files guarded from the upload on, the organization's settings (quota, links, search, comments), sharing by link for a file and a folder, comments, favourites and search through the text inside files. `drive_ui_e2e.mjs` drives the drive page in Chromium (about 40 checks: upload by button and by dropping, folders, the viewer panel, rename, sharing, favourites, comments, a link opened by someone else, versions, trash, search).

`chat_ui_e2e.mjs` drives the chat page in Chromium with two people (welcome pane, rail by team, live messages, presence, per-person unread badges, threads, forms, browse, narrow screen); it needs the built web app (`web/build`).

Needs `pip install websocket-client`. Build the binary (`cargo build -p aether` in `../../../aether`), have SurrealDB
running (user root, password root), then from this folder:

```sh
DB_PORT=8000 ./setup.sh     # configuration, core database, organization atr_org, users owner, bob and cy, the plugins
./serve.sh                  # the server on 127.0.0.1:17892; leave it running
python3 chat_e2e.py         # HTTP and WebSocket, 98 checks
node chat_ui_e2e.mjs        # the page in a browser, 31 checks (SHOTS=dir keeps screenshots)
```

An instance made before a kernel migration needs it applied: assigning a user to the organization does it (`--create-user ...`). `setup.sh` is for an empty database. `bob` and `cy` get the built-in role `user_manager`: a page needs a role of the
organization when none of the plugin's own exist, and this one grants nothing chat needs. After changing a plugin: bump
its version, `make`, `--load-plugins` it again and `--upgrade-plugin <name> --org atr_org`; restart the server after
changing roles.

`app_dir/` and `aether.toml` are throwaway; do not commit them.

`planner_e2e.py` drives `atrium_planner` over HTTP: plans and their buckets, task order and moves, assignment (members only), validation, late tasks and reminders, repeating tasks with their checklists, labels, people and the last-owner rule, and that the kernel's task conversation opens for plan members and not for strangers.

`people_e2e.py` drives `atrium_people` over HTTP: the directory and its search, who may edit a profile, reporting lines (no self, no loops, members only), the chart, and the fields the HR link keeps. `people_ui_e2e.mjs` drives the directory in Chromium (editing a profile, search, opening a profile, the editor, the chart). `hr_link_e2e.py` also checks that the HR fields reach the profile and follow a transfer; its transfer check is occasionally late (the event and the command sync race), so run it again if that one check fails.

`calendar_e2e.py` drives `atrium_calendar` over HTTP: personal and shared calendars and their roles, events that repeat and leave out days, invitations, answers and clashes, the agenda, availability, iCalendar files and reminders. `booking_e2e.py` drives `atrium_booking` (and the calendar functions it uses): offering time, free times, that the host's busy calendar takes slots away without showing why, booking, taken times, cancelling from either side and who may do what.

`public_booking_e2e.py` books as an outsider: it needs `[tenancy] org_resolution = "header"` in `aether.toml` (`setup.sh` sets it) and sends `X-Org-Slug`. `wiki_e2e.py`, `news_e2e.py` and `forms_e2e.py` drive `atrium_wiki`, `atrium_news` and `atrium_forms` over HTTP (spaces and their roles, links, history and putting a version back, search; authors, drafts, publishing, the feed, likes and read counts; questions of nine kinds, how answers are checked, one answer each, anonymous forms, results and the CSV). Every one also checks that the kernel's chatter on a page, post or event is closed to strangers. `forms_ui_e2e.mjs` fills a form in Chromium; `pages_ui_e2e.mjs` walks the wiki, news and booking pages with two people (editing a page, the history, a post written in the form, booking and cancelling a time). They need the built web app (`web/build`); build it with `npx vite build` in `aether/web`.

`mail_e2e.py` drives `atrium_mail` over HTTP and plays the mail provider: it makes the intake's secret and sets the mail domain (an administrator), makes boxes, posts mail to `/api/mail/atr_org/generic`, and checks that conversations reach only the people of the box, that `inbox+tag@` picks the box, that an answer carrying `[#key]` (or an `In-Reply-To` of a known message) joins the conversation, duplicates, taking/assigning/closing/reopening, search, and the chatter's access. The first mail after a restart can take a while (the server loads its plugins) and is retried. Sending a reply needs a mail bridge in the settings of the instance; without one the script checks that a failed reply leaves the conversation untouched and says so.

`mailbox_imap_e2e.py` does the same for a mail server that speaks IMAP: `fake_imap.py` plays both the IMAP server and the SMTP server the `imap` bridge sends through (the bridge's settings point at them, with `security = none`). `planner_ui_e2e.mjs` drives the planner board in Chromium: adding a task in a column, dragging a card to another column, the Blocked mark, the task page (what it waits for, picking who it is assigned to, renaming). `meet_e2e.py`, `office_e2e.py` and `mailbox_e2e.py` drive `atrium_meet`, the online editing of `atrium_drive` and `atrium_mailbox`. They switch the matching bridge on and set its settings, which needs the developer account `setup.sh` gives `owner` (an instance made before that: `aether -c aether.toml --grant-developer --username owner`, then sign in again). `meet_e2e.py` decodes the signed join links; `office_e2e.py` plays the editor server (it reads and saves through `/api/wopi/...` with the access token from the editor address, as Collabora would); `mailbox_e2e.py` starts `fake_jmap.py`, a small JMAP server with two accounts, and checks that each person reaches only their own mailbox, and that attachments are fetched, stored for that person alone, and sent. `pages_ui_e2e.mjs` also clicks through the meeting and mailbox pages. A bridge added after an instance was made needs `aether -c aether.toml --seed` and a server restart.
