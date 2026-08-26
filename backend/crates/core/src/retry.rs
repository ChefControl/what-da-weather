use std::future::Future;
use std::time::Duration;

/// Retry `op` up to `attempts` times with exponential backoff, but only while
/// `retryable` says the error is transient. Non-retryable errors return immediately.
pub async fn retry_if<T, E, F, Fut, R>(
    attempts: u32,
    base_delay: Duration,
    retryable: R,
    mut op: F,
) -> Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, E>>,
    R: Fn(&E) -> bool,
    E: std::fmt::Display,
{
    let mut attempt = 0;
    loop {
        match op().await {
            Ok(v) => return Ok(v),
            Err(e) => {
                attempt += 1;
                if attempt >= attempts || !retryable(&e) {
                    return Err(e);
                }
                let delay = base_delay * 2u32.saturating_pow(attempt - 1);
                tracing::warn!(error = %e, attempt, delay_ms = delay.as_millis() as u64, "retrying after transient failure");
                tokio::time::sleep(delay).await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    #[tokio::test]
    async fn returns_first_success() {
        let calls = AtomicU32::new(0);
        let res: Result<u32, String> = retry_if(
            3,
            Duration::from_millis(1),
            |_| true,
            || {
                calls.fetch_add(1, Ordering::SeqCst);
                async { Ok(7) }
            },
        )
        .await;
        assert_eq!(res.unwrap(), 7);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn retries_transient_then_succeeds() {
        let calls = AtomicU32::new(0);
        let res: Result<u32, String> = retry_if(
            3,
            Duration::from_millis(1),
            |_| true,
            || {
                let n = calls.fetch_add(1, Ordering::SeqCst);
                async move {
                    if n < 2 {
                        Err("boom".to_string())
                    } else {
                        Ok(42)
                    }
                }
            },
        )
        .await;
        assert_eq!(res.unwrap(), 42);
        assert_eq!(calls.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn does_not_retry_permanent_errors() {
        let calls = AtomicU32::new(0);
        let res: Result<u32, String> = retry_if(
            5,
            Duration::from_millis(1),
            |_| false,
            || {
                calls.fetch_add(1, Ordering::SeqCst);
                async { Err("permanent".to_string()) }
            },
        )
        .await;
        assert!(res.is_err());
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
}
