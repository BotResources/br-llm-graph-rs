mod instance;
mod late_input;

use std::sync::Arc;

use br_graph_contract::{
    CancelView, CreateFuture, Fields, Instance, InstanceId, Node, PROGRAMMATIC, State,
};

pub use self::late_input::LateInput;

type Predicate<F> = Box<dyn Fn(&State<F>) -> bool + Send + Sync>;

struct Rules<F: Fields> {
    name: String,
    ready: Predicate<F>,
    spent: Predicate<F>,
}

pub struct Merger<F: Fields> {
    rules: Arc<Rules<F>>,
    late_input: LateInput,
}

impl<F: Fields> Merger<F> {
    pub fn new(
        name: impl Into<String>,
        ready: impl Fn(&State<F>) -> bool + Send + Sync + 'static,
        spent: impl Fn(&State<F>) -> bool + Send + Sync + 'static,
    ) -> Self {
        Self {
            rules: Arc::new(Rules {
                name: name.into(),
                ready: Box::new(ready),
                spent: Box::new(spent),
            }),
            late_input: LateInput::default(),
        }
    }

    #[must_use]
    pub fn late_input(mut self, late_input: LateInput) -> Self {
        self.late_input = late_input;
        self
    }
}

impl<F: Fields> Node<F> for Merger<F> {
    fn name(&self) -> &str {
        &self.rules.name
    }

    fn kind(&self) -> &str {
        PROGRAMMATIC
    }

    fn subkind(&self) -> &str {
        "merger"
    }

    fn create(&self, cancel: CancelView, id: InstanceId) -> CreateFuture<'_, F> {
        let rules = Arc::clone(&self.rules);
        let late_input = self.late_input;
        Box::pin(async move {
            let (instance, reader, sender) = Instance::channel(id);
            tokio::spawn(instance::run(rules, late_input, reader, sender, cancel));
            Ok(instance)
        })
    }
}
