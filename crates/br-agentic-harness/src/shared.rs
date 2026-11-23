use crate::mail_set::MailSet;
use crate::runs::Runs;

#[derive(Default)]
pub(crate) struct Shared {
    pub(crate) mails: MailSet,
    pub(crate) runs: Runs,
}
