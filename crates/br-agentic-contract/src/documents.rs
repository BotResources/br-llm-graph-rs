use crate::error::PortError;
use crate::ids::MailId;
use crate::inbox::{InboxContext, InboxPort};
use crate::mail::MailDocument;
use crate::session::Session;

pub async fn mail_documents(
    session: &Session<'_>,
    inbox: &dyn InboxPort,
    context: &InboxContext,
    mail: MailId,
) -> Result<Vec<MailDocument>, PortError> {
    if !session.holds_incoming(mail.entry_id()) {
        return Err(PortError::refused(
            "the conversation holds no mail with that id",
        ));
    }
    inbox.documents(context, mail).await
}
