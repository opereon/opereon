#![feature(box_syntax, vec_remove_item, never_type)]
extern crate tokio;
extern crate tokio_codec;
extern crate tokio_signal;
extern crate tokio_core;
extern crate tokio_udp;

extern crate bytes;
extern crate uuid;
extern crate chrono;
extern crate futures;
extern crate interfaces;

#[macro_use]
extern crate serde_derive;
extern crate serde;
extern crate serde_json;

extern crate actix;

extern crate rmp_serde as rmps;
extern crate core;
extern crate hostname;
extern crate toml;
#[macro_use]
extern crate log;

extern crate kg_io;

use kg_io::*;

mod server;
mod ifaces;
mod codec;
mod node;
mod connection;
mod msgs;
mod discovery;
mod config;

pub use config::Config;
pub use discovery::{Discovery, DiscoveryRef};
pub use server::{Message, NodeId, ServerRef, Server, MessageResponse};
pub use msgs::{MessageReceived, NodeUnreachable, NodeReachable, Subscribe, Unsubscribe};
pub use codec::JsonCodec;
pub use node::NodeInfo;
pub use ifaces::*;
