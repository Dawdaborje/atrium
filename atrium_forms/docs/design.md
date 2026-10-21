# atrium_forms: design

Surveys and sign-up sheets for the organization, written in Rust. Filling a form uses the `<questionnaire>` widget (see
the pages documentation), because the questions of a form are data and not fixed page XML.

## Models

| Model | Holds |
|---|---|
| `frm_form` | `owner`, `title`, `description`, `status` (`draft`, `open`, `closed`), `anonymous`, `once`, `closes_at`, `thanks`, `edited_at` |
| `frm_question` | `form`, `position`, `label`, `kind`, `options`, `required`, `help` |
| `frm_response` | `form`, `user` (empty when anonymous), `answers` (`{ question key: answer }`), `submitted_at` |
| `frm_done` | `form`, `user`: who answered, kept apart from the answers so an anonymous form can still be answered once |

All four are closed by their rules (`via:self`).

## Kinds of question

`text` (one line, 500 characters), `long` (5,000), `choice` (one of the options), `dropdown` (the same, shown as a list),
`multi` (several), `rating` (1 to 5), `yes_no`, `number`, `date`. Answers are checked on the server against the question: a
choice must be one of the options (case is fixed to the option's), a required question must be answered, and an answer to a
question that does not exist is refused.

## Lifecycle

A form is a draft until its owner opens it (at least one question is needed). Open forms are listed for everyone in the
organization; a form past its closing date is closed even if nobody pressed Close. Once answers exist the form keeps its
questions, kinds and choices (rewording is allowed), cannot change between named and anonymous, and cannot go back to a
draft. Deleting a form deletes its questions and answers.

## Who sees what

The owner and administrators build the form and read results and responses. Everyone else sees an open form and nothing
of a draft. With `anonymous` the answer row has no user; the plugin keeps a separate "has answered" row, so the owner
cannot tell who said what (timing of submission is the one trace left, in the response time).

## Results

`results` sums each question as Markdown (counts and percentages per choice, an average for ratings and numbers, the latest
20 texts). `responses` lists single responses; `responses_csv` gives a CSV with one column for each question, where cells
that start with `=`, `+`, `-` or `@` are quoted so a spreadsheet does not run them.

## Known limits

Only people in the organization answer (no public link: that waits for the sharing proposal). No branching ("if yes, ask…"),
no file upload questions, no editing a submitted response, no quiz scoring, no copy of a form. Times are UTC.

## Tests

`tests/forms_e2e.py` (HTTP). Unit tests (`cargo test`) cover answer checking, summaries and CSV quoting.
