use tokio::sync::broadcast::{self, Sender};
use tokio::task::JoinSet;
use tokio_stream::StreamExt;
use tracing::{error, info};
use std::sync::Arc;

use crate::types::{Collector, Executor, Strategy};

/// The main engine of Artemis. This struct is responsible for orchestrating the
/// data flow between collectors, strategies, and executors.
pub struct Engine<E, A>
where
E: Send + Clone + 'static + std::fmt::Debug,
A: Send + Clone + 'static + std::fmt::Debug, {
    /// The set of collectors that the engine will use to collect events.
    collectors: Vec<Box<dyn Collector<E>>>,

    /// The set of strategies that the engine will use to process events.
    strategies: Vec<Arc<&'static dyn Strategy<E, A>>>,

    /// The set of executors that the engine will use to execute actions.
    executors: Vec<Box<dyn Executor<A>>>,
}

impl<E, A> Engine<E, A>
where
E: Send + Clone + 'static + std::fmt::Debug,
A: Send + Clone + 'static + std::fmt::Debug, {
    pub fn new() -> Self {
        Self {
            collectors: vec![],
            strategies: vec![],
            executors: vec![],
        }
    }
}

impl<E, A> Default for Engine<E, A>
where
E: Send + Clone + 'static + std::fmt::Debug,
A: Send + Clone + 'static + std::fmt::Debug, {
    fn default() -> Self {
        Self::new()
    }
}

impl<E, A> Engine<E, A>
where
    E: Send + Clone + 'static + std::fmt::Debug,
    A: Send + Clone + 'static + std::fmt::Debug,
{
    /// Adds a collector to be used by the engine.
    pub fn add_collector(&mut self, collector: Box<dyn Collector<E>>) {
        self.collectors.push(collector);
    }

    /// Adds a strategy to be used by the engine.
    pub fn add_strategy(&mut self, strategy: Arc<&'static dyn Strategy<E, A>>) {
        self.strategies.push(strategy);
    }

    /// Adds an executor to be used by the engine.
    pub fn add_executor(&mut self, executor: Box<dyn Executor<A>>) {
        self.executors.push(executor);
    }

    /// The core run loop of the engine. This function will spawn a thread for
    /// each collector, strategy, and executor. It will then orchestrate the
    /// data flow between them.
    pub async fn run(self) -> Result<JoinSet<()>, Box<dyn std::error::Error>> {
        let (event_sender, _): (Sender<E>, _) = broadcast::channel(102400);
        let (action_sender, _): (Sender<A>, _) = broadcast::channel(512);

        let mut set = JoinSet::new();

        // Spawn executors in separate threads.
        for executor in self.executors {
            let mut receiver = action_sender.subscribe();
            set.spawn(async move {
                info!("starting executor... ");
                loop {
                    match receiver.recv().await {
                        Ok(action) => match executor.execute(action).await {
                            Ok(_) => {}
                            Err(e) => error!("error executing action: {}", e),
                        },
                        Err(e) => error!("error receiving action: {}", e),
                    }
                }
            });
        }

        // Spawn strategies in separate threads.
        for strategy in self.strategies {
            let mut event_receiver = event_sender.subscribe();
            let action_sender = action_sender.clone();
            let event_sender = event_sender.clone();
            strategy.sync_state().await?;
            strategy.set_action_sender(action_sender).await?;
            strategy.set_event_sender(event_sender).await?;

            set.spawn(async move {
                info!("starting strategy... ");
                let mut receive_push_count = 0i32;
                loop {
                    match event_receiver.recv().await {
                        Ok(event) => {
                            strategy.push_event(event).await.unwrap();
                            if receive_push_count >= 15000 {
                                info!("receive some events and push into list");
                                receive_push_count = 0;
                            } else {
                                receive_push_count += 1;
                            }
                        },
                        Err(e) => error!("error receiving event: {}", e),
                    }
                }
            });
        }

        // Spawn collectors in separate threads.
        for collector in self.collectors {
            let event_sender = event_sender.clone();
            set.spawn(async move {
                info!("starting collector... ");
                let mut event_stream = collector.get_event_stream().await.unwrap();
                while let Some(event) = event_stream.next().await {
                    match event_sender.send(event) {
                        Ok(_) => {}
                        Err(e) => error!("error sending event: {}", e),
                    }
                }
                info!("stoping collector... ");
            });
        }

        Ok(set)
    }
}
