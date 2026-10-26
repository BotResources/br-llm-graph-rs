mod child;
mod error;
mod initial;
mod instance;
mod scope;
mod work;

use std::num::NonZeroUsize;
use std::sync::Arc;

use br_graph_contract::{
    CancelView, CreateFuture, DEFAULT_STATE_LIMIT, Fields, Instance, InstanceId, Node,
    PROGRAMMATIC, State,
};

pub use self::error::MapReduceError;

type Map<F, G> = Box<dyn Fn(&State<F>) -> Vec<G> + Send + Sync>;
type Reduce<F, G> = Box<dyn Fn(&[State<G>]) -> <F as Fields>::Update + Send + Sync>;

struct Rules<F: Fields, G: Fields> {
    name: String,
    map: Map<F, G>,
    internal_node: Arc<dyn Node<G>>,
    limit: NonZeroUsize,
    reduce: Reduce<F, G>,
}

pub struct MapReduce<F: Fields, G: Fields> {
    rules: Arc<Rules<F, G>>,
    internal_size_limit: NonZeroUsize,
}

impl<F: Fields, G: Fields> MapReduce<F, G> {
    pub fn new(
        name: impl Into<String>,
        map: impl Fn(&State<F>) -> Vec<G> + Send + Sync + 'static,
        internal_node: Arc<dyn Node<G>>,
        limit: NonZeroUsize,
        reduce: impl Fn(&[State<G>]) -> F::Update + Send + Sync + 'static,
    ) -> Self {
        Self {
            rules: Arc::new(Rules {
                name: name.into(),
                map: Box::new(map),
                internal_node,
                limit,
                reduce: Box::new(reduce),
            }),
            internal_size_limit: DEFAULT_STATE_LIMIT,
        }
    }

    #[must_use]
    pub fn internal_size_limit(mut self, internal_size_limit: NonZeroUsize) -> Self {
        self.internal_size_limit = internal_size_limit;
        self
    }
}

impl<F: Fields, G: Fields> Node<F> for MapReduce<F, G> {
    fn name(&self) -> &str {
        &self.rules.name
    }

    fn kind(&self) -> &str {
        PROGRAMMATIC
    }

    fn subkind(&self) -> &str {
        "map_reduce"
    }

    fn create(&self, cancel: CancelView, id: InstanceId) -> CreateFuture<'_, F> {
        let rules = Arc::clone(&self.rules);
        let internal_size_limit = self.internal_size_limit;
        Box::pin(async move {
            let (instance, reader, sender) = Instance::channel(id);
            tokio::spawn(instance::run(
                rules,
                internal_size_limit,
                reader,
                sender,
                cancel,
            ));
            Ok(instance)
        })
    }
}
