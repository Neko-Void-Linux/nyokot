// Errors are cold paths; boxing them would only add noise.
#![allow(clippy::result_large_err)]

pub mod adapters;
pub mod bus;
pub mod command_exec;
pub mod commands;
pub mod config;
pub mod db;
pub mod error;
pub mod github;
pub mod notifier;
pub mod router;
pub mod umf;
pub mod webhook;
