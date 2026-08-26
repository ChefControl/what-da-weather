//! Shared library for what-da-weather: domain types, activity rules engine,
//! weather provider, LLM client, RabbitMQ publisher and Prometheus metrics.

pub mod config;
pub mod event;
pub mod llm;
pub mod metrics;
pub mod publish;
pub mod retry;
pub mod rules;
pub mod weather;
