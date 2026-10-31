use br_llm_clients_core::StreamSink;

use crate::future::PortFuture;

pub type ResponseStream = Box<dyn StreamSink>;

pub trait ResultStream: Send {
    fn send<'a>(&'a mut self, fragment: String) -> PortFuture<'a, ()>;
}

pub type ResultStreamBox = Box<dyn ResultStream>;
