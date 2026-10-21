//! Atrium forms: surveys and sign-up sheets for the organization. The owner builds questions, opens the form, and reads
//! the results; everyone else fills it in (once, if the form says so), by name or anonymously. See docs/design.md.

mod answers;
mod common;
mod forms;
mod questions;

use aether_sdk::prelude::*;

handler! {
    // ---- forms
    fn my_forms(_input: Empty) -> Vec<Value> { forms::my_forms() }
    fn forms_to_fill(_input: Empty) -> Vec<Value> { forms::forms_to_fill() }
    fn form_info(input: forms::FormOnly) -> Value { forms::form_info(input) }
    fn form_header(input: forms::FormOnly) -> Value { forms::form_header(input) }
    fn create_form(input: forms::NewForm) -> Value { forms::create_form(input) }
    fn update_form(input: forms::UpdateForm) -> Value { forms::update_form(input) }
    fn open_form(input: forms::FormOnly) -> Value { forms::set_status(input, "open") }
    fn close_form(input: forms::FormOnly) -> Value { forms::set_status(input, "closed") }
    fn reopen_as_draft(input: forms::FormOnly) -> Value { forms::set_status(input, "draft") }
    fn delete_form(input: forms::FormOnly) -> Value { forms::delete_form(input) }
    // ---- questions
    fn form_questions(input: forms::FormOnly) -> Vec<Value> { questions::form_questions(input) }
    fn get_question(input: questions::QuestionOnly) -> Value { questions::get_question(input) }
    fn add_question(input: questions::NewQuestion) -> Value { questions::add_question(input) }
    fn update_question(input: questions::UpdateQuestion) -> Value { questions::update_question(input) }
    fn delete_question(input: questions::QuestionOnly) -> Value { questions::delete_question(input) }
    fn question_up(input: questions::QuestionOnly) -> Value { questions::shift(input, -1) }
    fn question_down(input: questions::QuestionOnly) -> Value { questions::shift(input, 1) }
    // ---- answering and results
    fn questionnaire(input: forms::FormOnly) -> Value { answers::questionnaire(input) }
    fn submit_response(input: answers::Submit) -> Value { answers::submit_response(input) }
    fn results(input: forms::FormOnly) -> Value { answers::results(input) }
    fn responses(input: forms::FormOnly) -> Vec<Value> { answers::responses(input) }
    fn delete_response(input: answers::ResponseOnly) -> Value { answers::delete_response(input) }
    fn responses_csv(input: forms::FormOnly) -> Value { answers::responses_csv(input) }
}
