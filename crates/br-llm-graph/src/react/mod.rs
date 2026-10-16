//! The parts of an agent built on br-llm-messages and br-llm-clients-core:
//! `LlmCall`, a node that makes one model call and gives back its answer as
//! data, and two agents, `ReactAgent` and `GeneratorCritic`, each a graph to
//! call with `SubGraph` or run on its own.
//!
//! A model-calling node is built from what the runner gives it (`Given`): its
//! model (`Model`, a `Client` of br-llm-clients-core), its system (`Prompt`)
//! and its history (`History`), each a value or a function of the node's
//! context (`RequestContext`) declaring the keys it reads (`Reads`). The tools
//! (`Tool`, judged by an optional `Hook`) and the thread store (`ThreadStore`)
//! are ports the host implements. An agent built with a thread store is
//! durable: it works in a thread and resumes from it; without one it works in
//! memory, for the current call only.
//!
//! Helpers for a node the host writes: `wire` renders a conversation for an
//! author, `complete` calls a client with the observer as sink, `structured`
//! reads the first structured block of a step into a type, `pending_calls`
//! gives the pending calls of an author's last turn.

mod agent_answer;
mod agent_build;
mod agent_call;
mod bounds;
mod check;
mod critic_view;
mod critic_writer;
mod critique;
mod declarations;
mod describe;
mod duo;
mod enter;
mod generator_critic;
mod generator_critic_build;
mod generator_critic_names;
mod generator_critic_nodes;
mod generator_critic_routing;
mod given;
mod helpers;
mod hook;
mod llm_call;
mod llm_call_check;
mod llm_node;
mod llm_node_target;
mod react_agent;
mod react_loop;
mod rehydrate;
mod round_limit;
mod round_limit_marks;
mod round_writer;
mod rule_check;
mod seats;
mod sink;
mod thread_store;
mod tool;
mod tool_answer;
mod tool_gate;
mod tool_node;
mod turn_clients;
mod turn_ids;
mod turn_writer;
mod verdict;

#[cfg(test)]
mod declared_edges_tests;
#[cfg(test)]
mod generator_critic_bounds_tests;
#[cfg(test)]
mod generator_critic_budget_tests;
#[cfg(test)]
mod generator_critic_caller_tests;
#[cfg(test)]
mod generator_critic_cut_tests;
#[cfg(test)]
mod generator_critic_durable_support;
#[cfg(test)]
mod generator_critic_durable_tests;
#[cfg(test)]
mod generator_critic_empty_tests;
#[cfg(test)]
mod generator_critic_entry_tests;
#[cfg(test)]
mod generator_critic_given_tests;
#[cfg(test)]
mod generator_critic_history_tests;
#[cfg(test)]
mod generator_critic_keys_tests;
#[cfg(test)]
mod generator_critic_override_tests;
#[cfg(test)]
mod generator_critic_prepare_resume_tests;
#[cfg(test)]
mod generator_critic_prepare_tests;
#[cfg(test)]
mod generator_critic_repaired_tests;
#[cfg(test)]
mod generator_critic_resume_tests;
#[cfg(test)]
mod generator_critic_retry_tests;
#[cfg(test)]
mod generator_critic_revision_entry_tests;
#[cfg(test)]
mod generator_critic_rule_context_tests;
#[cfg(test)]
mod generator_critic_rule_tests;
#[cfg(test)]
mod generator_critic_support;
#[cfg(test)]
mod generator_critic_tests;
#[cfg(test)]
mod generator_critic_text_tests;
#[cfg(test)]
mod generator_critic_tools_tests;
#[cfg(test)]
mod generator_critic_verdict_tests;
#[cfg(test)]
mod generator_critic_view_support;
#[cfg(test)]
mod generator_critic_view_tests;
#[cfg(test)]
mod llm_call_check_tests;
#[cfg(test)]
mod llm_call_conversation_tests;
#[cfg(test)]
mod llm_call_given_tests;
#[cfg(test)]
mod llm_call_map_tests;
#[cfg(test)]
mod llm_call_support;
#[cfg(test)]
mod llm_call_tests;
#[cfg(test)]
mod react_agent_bounds_tests;
#[cfg(test)]
mod react_agent_build_tests;
#[cfg(test)]
mod react_agent_caller_tests;
#[cfg(test)]
mod react_agent_entry_tests;
#[cfg(test)]
mod react_agent_given_tests;
#[cfg(test)]
mod react_agent_keys_tests;
#[cfg(test)]
mod react_agent_map_tests;
#[cfg(test)]
mod react_agent_session_tests;
#[cfg(test)]
mod react_agent_support;
#[cfg(test)]
mod react_agent_tests;
#[cfg(test)]
mod react_background_task_tests;
#[cfg(test)]
mod react_duplicate_tool_tests;
#[cfg(test)]
mod react_durable_answer_tests;
#[cfg(test)]
mod react_durable_entry_tests;
#[cfg(test)]
mod react_durable_resume_tests;
#[cfg(test)]
mod react_durable_support;
#[cfg(test)]
mod react_durable_write_tests;
#[cfg(test)]
mod react_helper_tests;
#[cfg(test)]
mod react_hook_failure_tests;
#[cfg(test)]
mod react_hook_node_support;
#[cfg(test)]
mod react_hook_resume_tests;
#[cfg(test)]
mod react_hook_slot_tests;
#[cfg(test)]
mod react_hook_stop_tests;
#[cfg(test)]
mod react_hook_support;
#[cfg(test)]
mod react_hook_tests;
#[cfg(test)]
mod react_host_node_tests;
#[cfg(test)]
mod react_llm_system_tests;
#[cfg(test)]
mod react_llm_tests;
#[cfg(test)]
mod react_loop_tests;
#[cfg(test)]
mod react_node_check_tests;
#[cfg(test)]
mod react_node_support;
#[cfg(test)]
mod react_node_tests;
#[cfg(test)]
mod react_partition_tests;
#[cfg(test)]
mod react_reserved_tests;
#[cfg(test)]
mod react_round_limit_refusal_tests;
#[cfg(test)]
mod react_round_limit_resume_tests;
#[cfg(test)]
mod react_round_limit_support;
#[cfg(test)]
mod react_round_limit_tests;
#[cfg(test)]
mod react_tool_concurrency_tests;
#[cfg(test)]
mod react_tool_result_tests;
#[cfg(test)]
mod react_turn_client_tests;
#[cfg(test)]
mod test_call;
#[cfg(test)]
mod test_clients;
#[cfg(test)]
mod test_clients_tests;
#[cfg(test)]
mod test_support;
#[cfg(test)]
mod test_tools;
#[cfg(test)]
mod turn_client_release_tests;
#[cfg(test)]
mod turn_client_scope_tests;
#[cfg(test)]
mod turn_ids_tests;

pub use generator_critic::GeneratorCritic;
pub(crate) use given::Origin;
pub use given::{
    AgentNode, Budgeted, GeneratorCriticNode, Given, History, LoopNode, Model, ModelNode, Prompt,
    Reads, RequestContext, Scheduled,
};
pub use helpers::{pending_calls, structured, wire};
pub use hook::{Hook, HookError, HookFuture, Judgment};
pub use llm_call::{LlmCall, LlmOutput, LlmRequest};
pub use react_agent::ReactAgent;
pub use round_limit::{OnLimit, RoundLimit};
pub use rule_check::{RuleCheck, Ruling};
pub use seats::{CriticHistory, CriticSeat, GeneratorSeat};
pub use sink::complete;
pub use thread_store::{
    MessageStatus, ThreadError, ThreadFuture, ThreadMessage, ThreadStore, ThreadView, TurnEnd,
};
pub use tool::{Tool, ToolError, ToolFuture, ToolOutput};
pub use verdict::{Verdict, VerdictOverride, VerdictReader};
