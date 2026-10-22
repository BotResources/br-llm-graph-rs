//! A durable agent's turn failed under a reply the client refused after
//! some of its events reached the thread: the state a resumed run would
//! find, in which the reply is asked for again, and the updates that bring
//! the agent's state there.

use crate::error::GraphError;
use crate::react::agent_call::Position;
use crate::react::given::LoopNode;
use crate::react::llm_node::LlmNode;
use crate::react::rehydrate::read;
use crate::react::thread_store::TurnEnd;
use crate::react::turn_writer::TurnWriter;
use crate::state::{State, Value};
use crate::update::Update;

impl<N: LoopNode> LlmNode<N> {
    /// Fails the turn `writer` writes, of rank `rank`, so the store repairs
    /// it, and gives the state a resumed run would find: `conversation` is
    /// the thread read again, and the call writes the turn of the next rank.
    /// For the generator-critic, the failed turn is not one of the call's
    /// turns: a reply asked for again is no generation.
    pub(crate) async fn reenter(
        &self,
        state: &State,
        writer: &TurnWriter<'_>,
        rank: usize,
    ) -> Result<State, GraphError> {
        writer.end(TurnEnd::Failed).await?;
        let view = read(writer.store, &writer.thread).await?;
        let mut next = state
            .derive(&self.key, Value::conversation(view.conversation))?
            .derive(&self.call.position, Position::Writing(rank + 1).value())?;
        if let Some(turns) = &self.call.turns {
            let kept = next
                .list(turns)?
                .iter()
                .filter(|listed| !matches!(listed, Value::Str(id) if id == writer.turn.as_str()))
                .cloned()
                .collect();
            next = next.derive(turns, Value::list(kept))?;
        }
        Ok(next)
    }

    /// The updates of a reply used on the state `reentered` gives, when its
    /// turn was failed before it: that state's `conversation`, where its
    /// call stands and, for the generator-critic, the call's turns, then
    /// `updates`. Without it, `updates` as they are.
    pub(crate) fn reentered(
        &self,
        reentered: Option<&State>,
        updates: Vec<Update>,
    ) -> Result<Vec<Update>, GraphError> {
        let Some(state) = reentered else {
            return Ok(updates);
        };
        let set = |key: &crate::value::Key, value| Update::Set {
            key: key.clone(),
            value,
        };
        let mut all = vec![
            set(
                &self.key,
                Value::conversation(state.conversation(&self.key)?.clone()),
            ),
            set(
                &self.call.position,
                Value::int(state.int(&self.call.position)?),
            ),
        ];
        if let Some(turns) = &self.call.turns {
            all.push(set(turns, Value::list(state.list(turns)?.to_vec())));
        }
        all.extend(updates);
        Ok(all)
    }
}
