use super::connection::*;
use super::server::NodeId;
use super::server;
use super::node::NodeInfo;
use actix::prelude::*;

use tokio::net::TcpStream;
use std::net;


#[derive(Message)]
pub struct TcpConnect(pub TcpStream, pub net::SocketAddr);

/// Register session in server
#[derive(Message)]
pub struct RegisterConnection(pub ConnectionRef);

/// Remove session
#[derive(Message)]
pub struct UnregisterConnection {
    pub connection: ConnectionRef,
}

#[derive(Message, Debug)]
pub struct AddKnownNodes(pub Vec<NodeInfo>);

#[derive(Message, Debug)]
#[rtype(result = "Result<Vec<::node::NodeInfo>, ()>")]
pub struct GetKnownHosts;

#[derive(Message, Debug)]
#[rtype(result = "Result<Vec<::server::NodeId>, ()>")]
pub struct GetReachableHosts;

#[derive(Message)]
pub struct SendNetMessage {
    pub msg: server::NetMessage,
}

#[derive(Message, Debug)]
#[rtype(result = "Result<(), ::std::io::Error>")]
pub struct SendMessage {
    pub target: NodeId,
    pub msg: server::Message,
}

#[derive(Message, Debug)]
#[rtype(result = "Result<::server::Message, ::std::io::Error>")]
pub struct SendRequest {
    pub target: NodeId,
    pub msg: server::Message,
}

#[derive(Message, Debug)]
#[rtype(result = "Result<(), ::std::io::Error>")]
pub struct BroadcastMessage {
    pub msg: server::Message,
}

#[derive(Message, Debug)]
#[rtype(result = "Result<::connection::ConnectionRef, ::std::io::Error>")]
pub struct ConnectionConnect;

#[derive(Message, Debug)]
#[rtype(result = "Result<(), ::std::io::Error>")]
pub struct ConnectionProbe;

#[derive(Message, Debug)]
#[rtype(result = "Result<(), ::std::io::Error>")]
pub struct Discover;

#[derive(Message)]
pub struct Close;

#[derive(Message, Clone, Debug)]
#[rtype(result = "Option<::server::Message>")]
pub struct MessageReceived {
    pub msg: server::Message,
    pub sender: NodeId,
}

impl From<NotifyMsgRcvd> for MessageReceived {
    fn from(msg: NotifyMsgRcvd) -> Self {
        MessageReceived {
            sender: msg.sender,
            msg: msg.msg,
        }
    }
}

/// Notify that the node is reachable
#[derive(Message, Debug)]
pub struct NodeUnreachable {
    pub node: NodeId
}

/// Notify that the node is unreachable
#[derive(Message, Debug)]
pub struct NodeReachable {
    pub node: NodeId
}

#[derive(Message)]
pub struct NotifyMsgRcvd {
    pub msg: server::Message,
    pub sender: NodeId,
    pub ack: Option<u64>,
}

#[derive(Message)]
pub struct NotifyAckRcvd {
    pub msg: server::Message,
    pub ack: u64,
}

#[derive(Message)]
pub struct Subscribe<M>(pub Recipient<M>)
    where
        M: actix::Message + Send,
        M::Result: Send;

#[derive(Message)]
pub struct Unsubscribe<M>(pub Recipient<M>)
    where
        M: actix::Message + Send,
        M::Result: Send;

