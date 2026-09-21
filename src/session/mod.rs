use std::sync::Arc;

use br_llm_messages::UserInput;
use futures_util::StreamExt;

use crate::error::GraphError;
use crate::graph::{Context, Graph};
use crate::run::{Checkpoint, Cursor, Inbox, Message, Outcome, RunFailure, Sender, channel, run};
use crate::state::{Config, State};
use crate::value::Key;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Start {
    Now,
    OnInput,
}

pub enum Ended {
    Cancelled {
        checkpoint: Checkpoint,
    },
    Failed {
        checkpoint: Checkpoint,
        error: GraphError,
    },
}

pub struct Session {
    graph: Arc<Graph>,
    config: Config,
    state: State,
    context: Context,
    cursor: Option<Cursor>,
    inbox: Inbox,
    sender: Sender,
    paused: bool,
}

impl Session {
    pub fn new(graph: Arc<Graph>, config: Config, state: State, context: Context) -> Self {
        let (sender, inbox) = channel();
        Self {
            graph,
            config,
            state,
            context,
            cursor: None,
            inbox,
            sender,
            paused: false,
        }
    }

    pub fn resume(
        graph: Arc<Graph>,
        config: Config,
        checkpoint: Checkpoint,
        context: Context,
    ) -> Result<Self, GraphError> {
        graph.validate_cursor(&checkpoint.cursor)?;
        let cursor = if checkpoint.cursor.is_empty() {
            None
        } else {
            Some(checkpoint.cursor)
        };
        let (sender, inbox) = channel();
        Ok(Self {
            graph,
            config,
            state: checkpoint.state,
            context,
            cursor,
            inbox,
            sender,
            paused: false,
        })
    }

    pub fn sender(&self) -> Sender {
        self.sender.clone()
    }

    pub fn state(&self) -> &State {
        &self.state
    }

    pub fn checkpoint(&self) -> Checkpoint {
        Checkpoint::new(self.state.clone(), self.cursor.clone().unwrap_or_default())
    }

    pub async fn run_once(&mut self) -> Result<Outcome, GraphError> {
        let cursor = self.cursor.take();
        let state = std::mem::replace(&mut self.state, State::empty());
        let result = run(
            self.graph.as_ref(),
            &self.config,
            state,
            cursor,
            &self.context,
            &mut self.inbox,
        )
        .await;
        self.absorb(result)
    }

    fn absorb(&mut self, result: Result<Outcome, RunFailure>) -> Result<Outcome, GraphError> {
        match result {
            Ok(Outcome::Finished { state, end }) => {
                self.state = state.clone();
                self.cursor = None;
                self.paused = false;
                Ok(Outcome::Finished { state, end })
            }
            Ok(Outcome::Paused { checkpoint }) => {
                self.state = checkpoint.state.clone();
                self.cursor = Some(checkpoint.cursor.clone());
                self.paused = true;
                Ok(Outcome::Paused { checkpoint })
            }
            Ok(Outcome::Cancelled { checkpoint }) => {
                self.state = checkpoint.state.clone();
                self.cursor = Some(checkpoint.cursor.clone());
                self.paused = false;
                Ok(Outcome::Cancelled { checkpoint })
            }
            Err(RunFailure { checkpoint, error }) => {
                self.state = checkpoint.state;
                self.cursor = Some(checkpoint.cursor);
                Err(error)
            }
        }
    }

    fn apply_input(&mut self, key: Key, input: UserInput) -> Result<(), GraphError> {
        self.state
            .apply_batch(&[crate::update::Update::Input { key, input }])
    }

    pub async fn serve(mut self, start: Start) -> Ended {
        if let Start::Now = start
            && let Some(ended) = self.pump().await
        {
            return ended;
        }
        loop {
            let Some(message) = self.inbox.receiver.next().await else {
                return Ended::Cancelled {
                    checkpoint: self.checkpoint(),
                };
            };
            match message {
                Message::Input { key, input } => {
                    if let Err(error) = self.apply_input(key, input) {
                        return Ended::Failed {
                            checkpoint: self.checkpoint(),
                            error,
                        };
                    }
                    if self.paused {
                        continue;
                    }
                    if let Some(ended) = self.pump().await {
                        return ended;
                    }
                }
                Message::Pause => self.paused = true,
                Message::Resume => {
                    if self.paused {
                        self.paused = false;
                        if let Some(ended) = self.pump().await {
                            return ended;
                        }
                    }
                }
                Message::Cancel => {
                    return Ended::Cancelled {
                        checkpoint: self.checkpoint(),
                    };
                }
            }
        }
    }

    async fn pump(&mut self) -> Option<Ended> {
        match self.run_once().await {
            Ok(Outcome::Finished { .. }) | Ok(Outcome::Paused { .. }) => None,
            Ok(Outcome::Cancelled { checkpoint }) => Some(Ended::Cancelled { checkpoint }),
            Err(error) => Some(Ended::Failed {
                checkpoint: self.checkpoint(),
                error,
            }),
        }
    }
}

#[cfg(test)]
#[path = "session_tests.rs"]
mod tests;
