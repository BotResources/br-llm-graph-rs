use std::num::NonZeroUsize;

use br_agentic_contract::{
    Cursor, IncomingBatch, IncomingContext, MailDocument, MailId, NewMail, PortError, PortFuture,
};
use br_llm_messages::EntryId;

pub trait MailPort: Send + Sync {
    fn items<'a>(
        &'a self,
        context: &'a IncomingContext,
        after: Option<&'a Cursor>,
        limit: NonZeroUsize,
    ) -> PortFuture<'a, Result<IncomingBatch, PortError>>;

    fn mark_read<'a>(
        &'a self,
        context: &'a IncomingContext,
        item: EntryId,
    ) -> PortFuture<'a, Result<(), PortError>>;

    fn send<'a>(
        &'a self,
        context: &'a IncomingContext,
        mail: NewMail,
    ) -> PortFuture<'a, Result<(), PortError>>;

    fn documents<'a>(
        &'a self,
        context: &'a IncomingContext,
        mail: MailId,
    ) -> PortFuture<'a, Result<Vec<MailDocument>, PortError>>;
}
