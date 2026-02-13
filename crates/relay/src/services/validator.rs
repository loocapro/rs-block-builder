use builder_primitives::validator::{ValidatorSchedule, ValidatorScheduleSlotInfo};
use futures_util::{Future, FutureExt};
use parking_lot::Mutex;
use reth::tasks::TaskSpawner;
use std::{
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};
use tokio::{sync::oneshot, time::Interval};
use tracing::{debug, error};

use super::relay::{RelayHandle, RelayServiceErr};

const EPOCH_TIME_SECONDS: u64 = 32 * 12;

#[derive(Debug, Clone)]
pub struct ValidatorScheduleHandle {
    state: Arc<Mutex<ValidatorSchedule>>,
}

impl ValidatorScheduleHandle {
    /// Get validator info for given slot
    pub fn validator_info(&self, slot: u64) -> Option<ValidatorScheduleSlotInfo> {
        let schedule = self.state.lock();
        let vinfo = schedule.get_by_slot(slot);
        drop(schedule);
        vinfo
    }
}

pub struct ValidatorScheduleService<Tasks> {
    /// Task executor
    executor: Tasks,
    /// Interval for validator info requests
    interval: Interval,
    /// Handle for relay service
    relay_handle: RelayHandle,
    /// Communication between triggered load task and validator service
    state_receiver: Option<oneshot::Receiver<Result<ValidatorSchedule, RelayServiceErr>>>,
    /// Map of validator info per slot
    state: Arc<Mutex<ValidatorSchedule>>,
}

impl<Tasks> ValidatorScheduleService<Tasks>
where
    Tasks: TaskSpawner + Clone + 'static,
{
    pub fn new(executor: Tasks, relay_handle: RelayHandle) -> (Self, ValidatorScheduleHandle) {
        let interval = tokio::time::interval(std::time::Duration::from_secs(EPOCH_TIME_SECONDS));
        let state = Arc::new(Mutex::new(Default::default()));
        let handle = ValidatorScheduleHandle {
            state: state.clone(),
        };
        let service = Self {
            executor,
            interval,
            relay_handle,
            state_receiver: None,
            state,
        };
        (service, handle)
    }

    pub fn reset_state(&self, schedule: ValidatorSchedule) {
        let mut data = self.state.lock();
        *data = schedule;
        drop(data);
    }

    /// Get range of slots in current schedule
    pub fn get_slot_range(&self) -> (Option<u64>, Option<u64>) {
        let schedule = self.state.lock();
        let min = (*schedule).as_ref().keys().min().cloned();
        let max = (*schedule).as_ref().keys().max().cloned();
        drop(schedule);
        (min, max)
    }
}

impl<Tasks> Future for ValidatorScheduleService<Tasks>
where
    Tasks: TaskSpawner + Clone + 'static,
{
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();

        while this.interval.poll_tick(cx).is_ready() {
            let (tx, rx) = oneshot::channel();
            let relay_handle = this.relay_handle.clone();
            this.executor.spawn(Box::pin(async move {
                let schedule = relay_handle.load_validator_schedule().await;
                let _ = tx.send(schedule);
            }));
            debug!(target: "relay::validator::service", "Triggered new task loading validator schedule");
            this.state_receiver = Some(rx);
        }

        if let Some(mut rx) = this.state_receiver.take() {
            match rx.poll_unpin(cx) {
                Poll::Ready(Ok(Ok(schedule))) => {
                    this.reset_state(schedule);
                    debug!(target: "relay::validator::service", "Reset validator schedule with slot range [{:?}]",
                        this.get_slot_range(),
                    );
                }
                Poll::Ready(Ok(Err(err))) => {
                    error!(target: "relay::validator::service", ?err, "Failed to load validator schedule");
                    this.interval.reset_after(std::time::Duration::from_secs(5));
                }
                Poll::Ready(Err(err)) => {
                    error!(target: "relay::validator::service", ?err, "Failed to receive validator schedule");
                    this.interval.reset_after(std::time::Duration::from_secs(5));
                }
                Poll::Pending => {
                    this.state_receiver = Some(rx);
                }
            }
        }

        Poll::Pending
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::relay::RelayService;
    use builder_primitives::{blst::public_key::BlsPublicKey, run_mode::RunMode};
    use reth::tasks::TokioTaskExecutor;
    use std::time::Duration;
    use tokio::time::sleep;

    #[ignore]
    #[tokio::test]
    async fn test_validator_service() {
        let (relay_service, relay_handle) =
            RelayService::new(RunMode::Simulate, false, BlsPublicKey::default());
        tokio::spawn(relay_service);
        let (v_service, v_handle) =
            ValidatorScheduleService::new(TokioTaskExecutor::default(), relay_handle);
        tokio::spawn(v_service);
        sleep(Duration::from_secs(4)).await;

        let binding = v_handle.state.lock();
        let first_slot = *binding.as_ref().keys().min().expect("no slots found");
        drop(binding);

        let v_info = v_handle
            .validator_info(first_slot)
            .expect("expecte slot info");
        assert_eq!(v_info.slot, first_slot);
    }

    #[ignore]
    #[tokio::test]
    async fn test_devnet_validator_service() {
        let (relay_service, relay_handle) =
            RelayService::new(RunMode::Simulate, false, BlsPublicKey::default());
        tokio::spawn(relay_service);
        let (v_service, v_handle) =
            ValidatorScheduleService::new(TokioTaskExecutor::default(), relay_handle);
        let slot_range = v_service.get_slot_range();
        assert_eq!(slot_range, (None, None));
        tokio::spawn(v_service);
        sleep(Duration::from_secs(4)).await;

        assert!(v_handle.validator_info(0).is_none());
    }
}
