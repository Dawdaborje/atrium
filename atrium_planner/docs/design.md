# atrium_planner: design

Plans with buckets, tasks, checklists, labels and repeating tasks, written in Rust. A plan is a board of buckets; a task
sits in one bucket, has assignees, dates, a priority and a checklist, and a conversation (the kernel's chatter).

## Models

| Model | Holds |
|---|---|
| `planner_plan` | `name`, `description`, `archived`, `created_by` |
| `planner_member` | `plan`, `user`, `role` (`owner` or `member`) |
| `planner_bucket` | `plan`, `name`, `position` |
| `planner_label` | `plan`, `name` |
| `planner_task` | `plan`, `bucket`, `position`, `title`, `description`, `assignees`, `start`, `due`, `priority`, `progress`, `labels`, `recurrence`, `check_total`, `check_done`; the `*_names`, `*_text` and `*_keys` fields repeat names and keys as text so lists show and search them |
| `planner_check` | `task`, `text`, `done`, `position` |

Rules let only the plugin's own functions touch these (`via:self`). The one exception is read access to `planner_task`
for people whose plan list holds the task's plan (`"plan": "$my_plans"`, answered by `rule_var_my_plans`); that is what lets
the kernel's chatter show a task's conversation to the plan's members and to nobody else.

## Access

Membership gates everything: a person who is not in a plan cannot see it, its buckets, labels, tasks or checklists
(the answer is "there is no such plan"). Members add and change tasks; owners also rename, archive and delete the plan,
manage buckets' deletion and people. A plan always keeps one owner. Assignees must be members; removing a member takes
them off the plan's tasks. An archived plan takes no new tasks and leaves the list.

## Ordering

`position` orders tasks in a bucket and buckets in a plan. `move_task(id, to, before?)` puts the task last in `to`, or in
front of `before`; `move_bucket` works the same way. Deleting a bucket moves its tasks to the first bucket left.

## Repeating tasks

A task with a repeat (daily, weekly, monthly, yearly) makes the next one when it is completed: same title, people, labels
and priority, the due and start dates moved on (months keep the day, or the last day of a shorter month), and the checklist
copied with every item unticked.

## Reminders

A schedule at 07:00 (`due-reminders`) tells each assignee of a task due today; assigning a task tells the new assignee.
Events announced: `task_created`, `task_assigned`, `task_completed`.

## Pages

`/planner` (plans), `/planner/new`, `/planner/{plan}` (the board: kanban with `columns`, quick add, drag to reorder),
`/planner/mine` (my tasks), `/planner/task/{id}` (the form, the `<checklist>` and the conversation), people and settings.

## Dependencies

A task can wait for other tasks of its plan (`blocked_by`, set on the task page as **Waits for**; at most 20; never itself and never a circle, however
indirect). While one of them is not completed the card shows **Blocked** and the task cannot be completed ("this task waits for: ..."). When the
last blocker is completed the task's people are told it can start. Deleting a task removes it from what waited for it. A repeating task's next
copy does not inherit its dependencies. `pick_tasks` lists the candidates.

## Known limits

No Gantt (waits for views and reports), no rules or approvals (automation), and a task has one bucket and one plan.

## Tests

`tests/planner_e2e.py` (HTTP, about 90 checks). A browser script for the board is still to write.

## Task page fields

"Assigned to", "Labels" and "Waits for" are `many2many` fields with a `source`: the page shows the picked rows by name (chips with a ×) and a search box that offers the rest (only the plan's people for "Assigned to"). `planner_ui_e2e.mjs` covers the board and these pickers in a browser.
