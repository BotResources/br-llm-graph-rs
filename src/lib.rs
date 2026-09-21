#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

#[cfg(test)]
mod scaffold {
    use br_llm_messages as _;
    use futures_channel::oneshot;
    use futures_util::FutureExt;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct Probe {
        ok: bool,
    }

    #[test]
    fn serde_round_trips() -> Result<(), serde_json::Error> {
        let encoded = serde_json::to_string(&Probe { ok: true })?;
        assert_eq!(serde_json::from_str::<Probe>(&encoded)?, Probe { ok: true });
        Ok(())
    }

    #[tokio::test]
    async fn a_node_result_reaches_its_join() {
        let (sender, receiver) = oneshot::channel::<Probe>();
        assert!(sender.send(Probe { ok: true }).is_ok());
        assert_eq!(receiver.map(Result::ok).await, Some(Probe { ok: true }));
    }
}
