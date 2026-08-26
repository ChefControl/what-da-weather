use std::time::Duration;

use lapin::options::{BasicPublishOptions, ConfirmSelectOptions, QueueDeclareOptions};
use lapin::types::FieldTable;
use lapin::{BasicProperties, Channel, Connection, ConnectionProperties};
use tokio::sync::Mutex;

use crate::event::EvaluationEvent;
use crate::metrics;

const PUBLISH_ATTEMPTS: u32 = 4;
const BACKOFF_BASE: Duration = Duration::from_millis(400);

/// RabbitMQ publisher implementing the producer side of the no-data-loss design
/// (DESIGN.md §6 layer 1): durable queue, persistent messages, publisher
/// confirms, bounded retry with reconnect between attempts.
pub struct Publisher {
    url: String,
    queue: String,
    channel: Mutex<Option<Channel>>,
}

impl Publisher {
    pub fn new(url: String, queue: String) -> Self {
        Self {
            url,
            queue,
            channel: Mutex::new(None),
        }
    }

    pub fn from_env() -> Self {
        Self::new(
            std::env::var("AMQP_URL")
                .unwrap_or_else(|_| "amqp://weather:weather@rabbitmq:5672/%2f".to_string()),
            std::env::var("AMQP_QUEUE").unwrap_or_else(|_| "weather-events".to_string()),
        )
    }

    pub async fn publish(&self, event: &EvaluationEvent) -> anyhow::Result<()> {
        let payload = serde_json::to_vec(event)?;
        let mut last_err = None;
        for attempt in 0..PUBLISH_ATTEMPTS {
            if attempt > 0 {
                tokio::time::sleep(BACKOFF_BASE * 2u32.saturating_pow(attempt - 1)).await;
            }
            match self.try_publish(&payload).await {
                Ok(()) => return Ok(()),
                Err(e) => {
                    tracing::warn!(error = %e, attempt = attempt + 1, "publish attempt failed");
                    metrics::PUBLISH_FAILURES.inc();
                    // Drop the channel so the next attempt reconnects from scratch.
                    *self.channel.lock().await = None;
                    last_err = Some(e);
                }
            }
        }
        Err(last_err.expect("at least one attempt"))
    }

    async fn try_publish(&self, payload: &[u8]) -> anyhow::Result<()> {
        let mut guard = self.channel.lock().await;
        if guard
            .as_ref()
            .map(|c| !c.status().connected())
            .unwrap_or(true)
        {
            *guard = Some(self.open_channel().await?);
        }
        let channel = guard.as_ref().expect("channel just ensured");

        let confirm = channel
            .basic_publish(
                "", // default exchange: routing key == queue name
                &self.queue,
                BasicPublishOptions::default(),
                payload,
                BasicProperties::default()
                    .with_delivery_mode(2) // persistent
                    .with_content_type("application/json".into()),
            )
            .await?
            .await?; // publisher confirm

        anyhow::ensure!(!confirm.is_nack(), "broker nacked the publish");
        Ok(())
    }

    async fn open_channel(&self) -> anyhow::Result<Channel> {
        let conn = Connection::connect(&self.url, ConnectionProperties::default()).await?;
        let channel = conn.create_channel().await?;
        channel
            .confirm_select(ConfirmSelectOptions::default())
            .await?;
        channel
            .queue_declare(
                &self.queue,
                QueueDeclareOptions {
                    durable: true,
                    ..Default::default()
                },
                FieldTable::default(),
            )
            .await?;
        Ok(channel)
    }
}
