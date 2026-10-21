//! Atrium planner: plans with buckets and tasks, assignment, due dates, checklists, repeating tasks and reminders.
//!
//! A plan belongs to its members; the owner manages it. Tasks live in buckets (the columns of a board). A task's
//! conversation is the kernel's chatter, so comments, followers and the history of changes come with it. See
//! docs/design.md.

mod access;
mod common;
mod plans;
mod reminders;
mod tasks;
mod views;

use aether_sdk::prelude::*;

handler! {
    // ---- plans
    fn my_plans(_input: Empty) -> Vec<Value> { views::my_plans() }
    fn create_plan(input: plans::NewPlan) -> Value { plans::create_plan(input) }
    fn edit_plan(input: plans::EditPlan) -> Value { plans::edit_plan(input) }
    fn delete_plan(input: plans::PlanOnly) -> Value { plans::delete_plan(input) }
    fn plan_header(input: plans::PlanOnly) -> Value { views::plan_header(input) }
    fn plan_info(input: plans::PlanOnly) -> Value { views::plan_info(input) }
    // ---- people
    fn plan_members(input: plans::PlanOnly) -> Vec<Value> { plans::plan_members(input) }
    fn add_plan_members(input: plans::AddMembers) -> Value { plans::add_plan_members(input) }
    fn remove_plan_member(input: plans::RemoveMember) -> Value { plans::remove_plan_member(input) }
    fn set_plan_role(input: plans::SetRole) -> Value { plans::set_plan_role(input) }
    // ---- buckets and labels
    fn board_buckets(input: plans::PlanOnly) -> Vec<Value> { views::board_buckets(input) }
    fn add_bucket(input: plans::NewBucket) -> Value { plans::add_bucket(input) }
    fn rename_bucket(input: plans::RenameBucket) -> Value { plans::rename_bucket(input) }
    fn delete_bucket(input: plans::DeleteBucket) -> Value { plans::delete_bucket(input) }
    fn move_bucket(input: plans::MoveBucket) -> Value { plans::move_bucket(input) }
    fn add_label(input: plans::NewLabel) -> Value { plans::add_label(input) }
    fn delete_label(input: plans::LabelOnly) -> Value { plans::delete_label(input) }
    // ---- tasks
    fn board_tasks(input: plans::PlanOnly) -> Vec<Value> { views::board_tasks(input) }
    fn get_task(input: tasks::TaskOnly) -> Value { views::get_task(input) }
    fn my_tasks(input: views::Mine) -> Vec<Value> { views::my_tasks(input) }
    fn create_task(input: tasks::NewTask) -> Value { tasks::create_task(input) }
    fn quick_add(input: tasks::QuickAdd) -> Value { tasks::quick_add(input) }
    fn update_task(input: tasks::UpdateTask) -> Value { tasks::update_task(input) }
    fn move_task(input: tasks::MoveIn) -> Value { tasks::move_task(input) }
    fn complete_task(input: tasks::TaskOnly) -> Value { tasks::complete_task(input) }
    fn reopen_task(input: tasks::TaskOnly) -> Value { tasks::reopen_task(input) }
    fn delete_task(input: tasks::TaskOnly) -> Value { tasks::delete_task(input) }
    // ---- checklist
    fn task_checklist(input: tasks::TaskOnly) -> Vec<Value> { tasks::checklist(input) }
    fn add_check(input: tasks::AddCheck) -> Value { tasks::add_check(input) }
    fn toggle_check(input: tasks::Toggle) -> Value { tasks::toggle_check(input) }
    fn remove_check(input: tasks::TaskOnly) -> Value { tasks::remove_check(input) }
    // ---- pickers on the pages
    fn pick_buckets(input: views::Pick) -> Vec<Value> { views::pick_buckets(input) }
    fn pick_tasks(input: views::Pick) -> Vec<Value> { views::pick_tasks(input) }
    fn pick_people(input: views::Pick) -> Vec<Value> { views::pick_people(input) }
    fn pick_labels(input: views::Pick) -> Vec<Value> { views::pick_labels(input) }
    fn pick_org_people(input: views::Pick) -> Vec<Value> { views::pick_org_people(input) }
    // ---- the morning nudge, and the record rule
    fn due_reminders(_input: Empty) -> Value { reminders::due_reminders() }
    fn rule_var_my_plans(_input: Empty) -> Vec<String> { views::rule_my_plans() }
}
