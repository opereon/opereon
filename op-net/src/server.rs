use actix::prelude::Request as ActixRequest;
use actix::prelude::*;

use tokio;
use tokio::net::TcpListener;
use tokio::net::TcpStream;
use tokio_udp::UdpSocket;

use uuid::Uuid;

use super::connection::*;
use super::node::NodeInfo;
use super::msgs::*;

use futures;
use futures::Future;
use futures::Stream;

use std::collections::HashMap;
use std::io;
use std::net::IpAddr;
use std::net::Ipv4Addr;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;

use actix::dev::ResponseChannel;
use config::Config;
use futures::sync::oneshot::Sender;
use Discovery;
use DiscoveryRef;

pub type NodeId = Uuid;

/// Type alias for application layer message
pub type Message = Vec<u8>;

type ConnectSharedFut =
    futures::future::Shared<Box<Future<Item = ConnectionRef, Error = io::Error>>>;

/// Enum representing response type
pub enum MessageResponse {
    /// No response will be send.
    None,

    /// Sync response.
    Sync(Message),

    /// Async response. Resolves to `Option<Message>`, should never fail.
    /// If resolved value == `None` no response will be send.
    Async(Box<Future<Item = Option<Message>, Error = !>>),
}

impl MessageResponse {
    pub fn new_async<F>(fut: F) -> MessageResponse
    where
        F: Future<Item = Option<Message>, Error = !> + 'static,
    {
        MessageResponse::Async(box fut)
    }
}

// Implement actix::MessageResponse for self::MessageResponse so we can use it as return value from actix handler
impl<A: Actor> actix::dev::MessageResponse<A, MessageReceived> for MessageResponse {
    fn handle<R: ResponseChannel<MessageReceived>>(
        self,
        _ctx: &mut <A as Actor>::Context,
        tx: Option<R>,
    ) {
        if let Some(tx) = tx {
            match self {
                MessageResponse::None => tx.send(None),
                MessageResponse::Sync(msg) => tx.send(Some(msg)),
                MessageResponse::Async(fut) => Arbiter::spawn(fut.then(move |res| {
                    let res = res.expect("MessageResponse::Async() should never fail!");
                    tx.send(res);
                    futures::future::ok(())
                })),
            }
        }
    }
}

/// Enum representing wire protocol message
#[derive(Debug, Deserialize, Serialize)]
pub enum NetMessage {
    /// Message from remote node. Contains application layer data and optional acknowledgement.
    /// `ack` == Some() means that this message requires response (`Ack` with corresponding id).
    Message {
        payload: Message,
        ack: Option<u64>,
    },

    /// Acknowledgement for `Message`. Contains application layer data and ack of corresponding `Message`.
    Ack {
        payload: Message,
        ack: u64,
    },
    Handshake(NodeInfo),
    HandshakeOk,
    Probe,
    ProbeResp,
    Ping,
    Pong,
}

impl NetMessage {
    pub fn new_message(payload: Message) -> NetMessage {
        NetMessage::Message { payload, ack: None }
    }
    pub fn new_request(payload: Message, ack: u64) -> NetMessage {
        NetMessage::Message {
            payload,
            ack: Some(ack),
        }
    }
}

pub struct Server {
    /// Established connections
    connections: HashMap<NodeId, Vec<ConnectionRef>>,

    /// All known nodes
    known_nodes: HashMap<NodeId, NodeInfo>,

    /// Addresses of directly available nodes.
    /// Updated by probing `known_nodes`.
    reachable_nodes: HashMap<NodeId, SocketAddr>,

    /// Cloneable futures representing establishing connection result.
    connecting: HashMap<NodeId, ConnectSharedFut>,

    /// Subscriber of `MessageReceived` event.
    message_received_sub: Option<Recipient<MessageReceived>>,

    /// Subscriber of `NodeReachable` event.
    node_reachable_sub: Option<Recipient<NodeReachable>>,

    /// Subscriber of `NodeUnreachable` event.
    node_unreachable_sub: Option<Recipient<NodeUnreachable>>,

    /// Next ack id to use.
    next_ack: u64,

    /// Map of awaiting acks. Used to notify about ack received.
    acks: HashMap<u64, Sender<Message>>,

    this_node: Arc<NodeInfo>,
    config: Arc<Config>,
    discovery: DiscoveryRef,
}

impl Actor for Server {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Context<Server>) {
        let fut = self
            .discovery
            .discover()
            .into_actor(self)
            .then(
                |res: Result<Result<(), io::Error>, MailboxError>, _, ctx: &mut Context<Server>| {
                    // check discovery result, if error occurred, stop server
                    if res.is_err() {
                        error!("Discovery mailbox error, stopping...");
                        ctx.stop();
                        return actix::fut::err(());
                    }

                    let discovery_res = res.unwrap();

                    if discovery_res.is_ok() {
                        return actix::fut::ok(());
                    }

                    let err = discovery_res.unwrap_err();

                    error!("Discovery error, stopping = {:?}", err);
                    ctx.stop();
                    return actix::fut::err(());
                },
            ).and_then(|_, server: &mut Server, ctx: &mut Context<Server>| {
                // Discovery finished successfully, start probing nodes
                let interval = Duration::from_millis(server.config().probe_node_interval());

                server.probe_nodes(ctx);

                ctx.run_interval(interval, |server, ctx| {
                    server.probe_nodes(ctx);
                });

                info!("H2H server: Started");
                actix::fut::ok(())
            });

        ctx.spawn(fut);
    }

    fn stopped(&mut self, _ctx: &mut <Self as Actor>::Context) {
        self.discovery.stop();
        self.message_received_sub = None;
        self.node_unreachable_sub = None;
        self.node_reachable_sub = None;
        for (_id, conns) in self.connections.drain().into_iter() {
            for conn in conns.iter() {
                conn.close();
            }
        }
        info!("H2H server: Stopped");
    }
}

impl Server {
    pub fn this_node(&self) -> &Arc<NodeInfo> {
        &self.this_node
    }

    pub fn config(&self) -> &Arc<Config> {
        &self.config
    }

    pub fn known_nodes(&self) -> &HashMap<NodeId, NodeInfo> {
        &self.known_nodes
    }

    /// Send message to single node
    pub fn send_message(
        &mut self,
        node_id: &NodeId,
        data: Message,
        ctx: &mut Context<Server>,
    ) -> Box<dyn Future<Item = (), Error = io::Error>> {
        self.send_net_message(node_id, NetMessage::new_message(data), ctx)
    }

    /// Send request
    pub fn send_request(
        &mut self,
        node_id: &NodeId,
        data: Message,
        ctx: &mut Context<Server>,
    ) -> Box<dyn ActorFuture<Item = Message, Error = io::Error, Actor = Server>> {
        let ack_id = self.next_ack;
        self.next_ack += 1;

        let msg = NetMessage::new_request(data, ack_id);

        let (tx, rx) = futures::oneshot::<Message>();

        self.add_ack(ack_id, tx);

        let fut = self.send_net_message(node_id, msg, ctx).and_then(|_| {
            rx.map_err(|_err| {
                // sender can be dropped only after response received, timeout reached or server dropped.
                io::Error::new(io::ErrorKind::Other, "Server stopped")
            })
        });

        let fut = tokio::timer::Deadline::new(
            fut,
            Instant::now() + Duration::from_millis(self.config().default_ack_timeout()),
        ).map_err(|_| io::Error::new(io::ErrorKind::Other, "Response timeout reached"))
        .into_actor(self)
        .then(move |res, server, _ctx| {
            server.remove_ack(ack_id);
            actix::fut::result(res)
        });

        box fut
    }

    /// Broadcast message to all known nodes except itself
    pub fn broadcast_message(
        &mut self,
        msg: Message,
        ctx: &mut Context<Server>,
    ) -> Box<dyn Future<Item = (), Error = io::Error>> {
        let mut futs = Vec::with_capacity(self.known_nodes().len());

        let ids = self
            .known_nodes()
            .keys()
            .filter(|id| *id != self.this_node().id())
            .map(|id| id.clone())
            .collect::<Vec<NodeId>>();

        for id in &ids {
            futs.push(self.send_message(id, msg.clone(), ctx))
        }

        box futures::future::join_all(futs).map(|_results| ())
    }

    // FIXME remove later https://github.com/actix/actix/issues/132
    #[allow(dead_code)]
    fn get_connection(&self, id: &NodeId) -> Option<ConnectionRef> {
        match self.connections.get(id) {
            Some(conns) => {
                if conns.len() > 0 {
                    Some(conns[0].clone())
                } else {
                    None
                }
            }
            None => None,
        }
    }

    fn start(config: Config) -> io::Result<ServerRef> {
        let tcp_listener = TcpListener::bind(config.listen_addr())?;

        let udp_socket = UdpSocket::bind(config.listen_addr())?;
        match config.multicast_addr() {
            IpAddr::V4(ref addr) => {
                let any_ifc: Ipv4Addr = "0.0.0.0".parse().unwrap();
                udp_socket.join_multicast_v4(addr, &any_ifc)?;
            }
            IpAddr::V6(ref addr) => {
                udp_socket.join_multicast_v6(addr, 0)?;
            }
        }

        let config = Arc::new(config);
        let this_node = config.this_node().clone();
        info!("Starting op-net server for node = {}", this_node.id());

        let srv = Server::create(|ctx| {
            ctx.add_stream(tcp_listener.incoming().map(|st| {
                let addr = st.peer_addr().unwrap();
                TcpConnect(st, addr)
            }));

            let server = ServerRef::new(ctx.address());
            let discovery = Discovery::start(udp_socket, config.clone(), server);

            let mut known_nodes = HashMap::new();

            known_nodes.insert(this_node.id().clone(), this_node.clone());
            Server {
                discovery,
                connections: HashMap::new(),
                known_nodes: known_nodes,
                message_received_sub: None,
                node_reachable_sub: None,
                node_unreachable_sub: None,
                connecting: HashMap::new(),
                reachable_nodes: HashMap::new(),
                this_node: Arc::new(this_node),
                next_ack: 0,
                acks: HashMap::new(),
                config,
            }
        });
        Ok(ServerRef::new(srv))
    }

    fn send_net_message(
        &mut self,
        node_id: &NodeId,
        msg: NetMessage,
        ctx: &mut Context<Server>,
    ) -> Box<dyn Future<Item = (), Error = io::Error>> {
        let srv = ServerRef::new(ctx.address());
        let config = self.config().clone();

        //        if let Some(conn) = self.get_connection(node_id) {
        //            conn.send_net_message(msg);
        //            return Box::new(futures::future::ok(()));
        //        }

        if !self.known_nodes.contains_key(node_id) {
            // Unknown node
            let f = futures::future::err(io::Error::new(
                io::ErrorKind::Other,
                format!("Unknown node : {}", node_id),
            ));
            return Box::new(f);
        }

        let node_id = node_id.clone();

        let connect =
            move |sock_addr: SocketAddr| -> Box<Future<Item = ConnectionRef, Error = io::Error>> {
                let f = TcpStream::connect(&sock_addr)
                    .and_then(move |stream| {
                        let conn =
                            ConnectionRef::create(false, node_id, stream, srv.clone(), config);
                        conn.connect().map_err(|_err| {
                            io::Error::new(io::ErrorKind::Other, "Actor mailbox error")
                        })
                    }).and_then(|res: Result<ConnectionRef, io::Error>| match res {
                        Ok(conn) => futures::future::ok(conn),
                        Err(err) => futures::future::err(err),
                    });

                Box::new(f)
            };

        fn send_msg(
            shared_fut: ConnectSharedFut,
            msg: NetMessage,
        ) -> Box<dyn Future<Item = (), Error = io::Error>> {
            let ret = shared_fut
                .and_then(move |conn| {
                    conn.send_net_message(msg);

                    // FIXME https://github.com/actix/actix/issues/132
                    conn.close();

                    futures::future::ok(())
                }).map_err(|err| {
                    // convert shared error to io::Error
                    use std::error::Error;
                    let error = io::Error::new(io::ErrorKind::Other, err.description());
                    error
                });
            return box ret;
        }

        if self.connecting.contains_key(&node_id) {
            let shared_fut = self.connecting.get(&node_id).unwrap().clone();
            return send_msg(shared_fut, msg);
        }

        match self.reachable_nodes.get(&node_id) {
            // reachable node found just open connection and send message
            Some(addr) => {
                let connect_fut = connect(*addr).shared();

                self.connecting.insert(node_id, connect_fut.clone());

                send_msg(connect_fut, msg)
            }
            // reachable node not found, try to probe node and send message to discovered address
            // if probe fails, return error
            None => {
                let connect_fut = self
                    .probe_node(self.known_nodes().get(&node_id).unwrap(), ctx)
                    .map_err(move |_| {
                        // unreachable node
                        io::Error::new(
                            io::ErrorKind::Other,
                            format!("Unreachable node : {}", node_id),
                        )
                    }).and_then(connect);

                let connect_fut: Box<
                    Future<Item = ConnectionRef, Error = io::Error>,
                > = box connect_fut;

                let connect_fut = connect_fut.shared();

                self.connecting.insert(node_id, connect_fut.clone());

                send_msg(connect_fut, msg)
            }
        }
    }

    fn notify_reachable_node(&self, id: NodeId) {
        if let Some(ref sub) = self.node_reachable_sub {
            if sub.do_send(NodeReachable { node: id }).is_err() {
                warn!("HostReachable subscriber dropped")
            }
        }
    }

    fn notify_unreachable_node(&self, id: NodeId) {
        if let Some(ref sub) = self.node_unreachable_sub {
            if sub.do_send(NodeUnreachable { node: id }).is_err() {
                warn!("HostUnreachable subscriber dropped")
            }
        }
    }

    /// Add established connection
    fn add_connection(&mut self, connection: ConnectionRef) {
        //        eprintln!("New connection established= {:?}", connection);

        let id = connection.id().clone();

        self.connecting.remove(&id);

        if self.connections.contains_key(&id) {
            let conns = self.connections.get_mut(&id).unwrap();
            conns.push(connection);
            if conns.len() != 1 {
                // TODO implement algorithm of closing duplicated connections
                warn!("Multiple connections to node = {} : {:#?}", id, conns);
            }
        } else {
            self.connections.insert(id.clone(), vec![connection]);
        }
    }

    /// Remove established connection (connection closed)
    fn remove_connection(&mut self, conn: ConnectionRef) {
        let mut remove_entry = false;
        self.connecting.remove(conn.id());

        {
            let conns = self.connections.get_mut(conn.id());

            if let Some(conns) = conns {
                conns.remove_item(&conn);
                remove_entry = conns.len() == 0;
            }
        }

        if remove_entry {
            self.connections.remove(conn.id());
        }
    }

    fn add_reachable_node(&mut self, id: NodeId, addr: SocketAddr) {
        if self.reachable_nodes.insert(id, addr).is_none() {
            self.notify_reachable_node(id);
            info!(
                "Adding new reachable node = {:?}: {:?}",
                id.to_string(),
                addr
            );
        };
    }

    fn remove_reachable_node(&mut self, id: &NodeId) {
        if self.reachable_nodes.remove(id).is_some() {
            self.notify_unreachable_node(*id);
            info!("Removing reachable node = {:?}", id.to_string());
        };
    }

    fn add_known_node(&mut self, node: NodeInfo) {
        let found = self.known_nodes.remove(node.id());

        if found.is_none() {
            info!(
                "New known node= {:?} : {:?}",
                node.id().to_string(),
                &node.ipv4
            );
            self.known_nodes.insert(node.id().clone(), node);
            return;
        }

        let found = found.unwrap();

        if found.time < node.time {
            info!("Known node updated= {:?}", &node);
            self.known_nodes.insert(node.id().clone(), node);
        } else {
            self.known_nodes.insert(found.id().clone(), found);
        }
    }

    fn add_ack(&mut self, ack_id: u64, sender: Sender<Message>) {
        debug_assert!(
            self.acks.insert(ack_id, sender).is_none(),
            "Duplicated ack id"
        )
    }

    fn remove_ack(&mut self, ack_id: u64) {
        self.acks.remove(&ack_id);
    }

    fn get_ack(&mut self, ack_id: &u64) -> Option<Sender<Message>> {
        self.acks.remove(ack_id)
    }

    fn probe_node_addr(
        &self,
        addr: &SocketAddr,
        node_id: &NodeId,
        ctx: &mut Context<Server>,
    ) -> impl Future<Item = SocketAddr, Error = ()> {
        trace!("Probing node address {}, {}", node_id, addr);
        let srv = ServerRef::new(ctx.address());
        let timeout = Instant::now() + Duration::from_millis(self.config().probe_node_timeout());

        let addr = addr.clone();
        let node_id = node_id.clone();
        let config = self.config().clone();
        let server = srv.clone();

        let connect_fut = TcpStream::connect(&addr)
            .map_err(|_err| {
                //cannot connect, so addr is unreachable
            }).and_then(move |stream| {
                let conn = ConnectionRef::create(false, node_id, stream, server, config);
                let probe_fut = conn
                    .clone()
                    .probe()
                    .map_err(|_err| warn!("Actor mailbox error probing node"))
                    .then(move |probe_result| {
                        // probe finished, no need to maintain this connection
                        conn.close();
                        match probe_result {
                            Ok(_) => futures::future::ok(addr),
                            Err(_err) => futures::future::err(()),
                        }
                    });

                probe_fut
            });

        // add connection timeout
        let connect_fut = tokio::timer::Deadline::new(connect_fut, timeout).map_err(|_| {
            // timeout reached or connection initialization error
        });
        connect_fut
    }

    /// Check availability of node ip addresses.
    ///
    /// Future completes with first reachable `SocketAddr` or error if there is no reachable address.
    fn probe_node(
        &self,
        node: &NodeInfo,
        ctx: &mut Context<Server>,
    ) -> Box<Future<Item = SocketAddr, Error = ()>> {
        let mut futs = Vec::with_capacity(node.ipv4.len());

        if node.ipv4.len() == 0 {
            return Box::new(futures::future::err(()));
        }

        for addr in node.ipv4.iter() {
            let addr: SocketAddr = (*addr).into();
            //            eprintln!("Sending probe to = {:?}", addr);
            let probe_addr_fut = self.probe_node_addr(&addr, node.id(), ctx);

            futs.push(probe_addr_fut)
        }

        let probe_node_fut = futures::select_ok(futs).and_then(|(addr, futs)| {
            // we don't care about remaining futures,
            // but they must be polled to completion to properly close opened connections

            // do not report remaining futures errors
            let futs = futs
                .into_iter()
                .map(|fut| fut.then(|_| futures::future::ok::<(), ()>(())));

            let remaining_futs =
                futures::future::join_all(futs).then(move |_| futures::future::ok(()));

            // spawn remaining futs
            Arbiter::spawn(remaining_futs);

            futures::future::ok(addr)
        });
        Box::new(probe_node_fut)
    }

    fn probe_nodes(&self, ctx: &mut Context<Server>) {
        for (id, node) in self.known_nodes.iter() {
            // do not probe current node
            if id == self.this_node().id() {
                continue;
            }

            let id = id.clone();

            if node.ipv4.len() == 0 {
                warn!("Node without ips detected! {:?}", node);
                continue;
            }

            // no need to probe connecting node
            if self.connections.contains_key(&id) || self.connecting.contains_key(&id) {
                continue;
            }

            // check if node already have reachable address
            if let Some(addr) = self.reachable_nodes.get(&id) {
                let node = node.clone();
                let f =
                    self.probe_node_addr(addr, &id, ctx)
                        .into_actor(self)
                        .then(move |res: Result<SocketAddr, ()>, server: &mut Server, ctx: &mut actix::Context<Server>| -> Box<ActorFuture<Item=(), Error=(), Actor=Server>>{
                            if res.is_ok() {
                                // node already in connection.reachable_nodes, nothing to do
                                return box actix::fut::ok::<(), (), Server>(());
                            }

                            // if previously reachable address is unreachable, check remaining addresses
                            box server.probe_node(&node, ctx)
                                .into_actor(server)
                                .and_then(move |addr, server: &mut Server, _| {
                                    server.add_reachable_node(id, addr);
                                    actix::fut::ok(())
                                })
                                .map_err(move |_err, server: &mut Server, _| {
                                    server.remove_reachable_node(&id);
                                })
                        });
                ctx.spawn(f);
                continue;
            }

            let f = self
                .probe_node(node, ctx)
                .into_actor(self)
                .map(move |addr, server: &mut Server, _| {
                    server.add_reachable_node(id, addr);
                }).map_err(move |_err, server: &mut Server, _| {
                    server.remove_reachable_node(&id);
                });
            ctx.spawn(f);
        }
    }
}

impl StreamHandler<TcpConnect, io::Error> for Server {
    fn handle(&mut self, msg: TcpConnect, ctx: &mut actix::Context<Self>) {
        let addr = ServerRef::new(ctx.address());

        // Set temporary id. Will be replaced with node.id during handshake phase
        ConnectionRef::create(true, Uuid::nil(), msg.0, addr, self.config().clone());
    }

    fn error(&mut self, err: io::Error, _ctx: &mut actix::Context<Self>) -> Running {
        error!("Stream error, closing {:?}", err);
        Running::Stop
    }
}

impl Handler<RegisterConnection> for Server {
    type Result = ();

    fn handle(&mut self, msg: RegisterConnection, _ctx: &mut Self::Context) {
        self.add_connection(msg.0);
    }
}

impl Handler<Close> for Server {
    type Result = ();

    fn handle(&mut self, _msg: Close, ctx: &mut Self::Context) {
        ctx.stop();
    }
}

impl Handler<UnregisterConnection> for Server {
    type Result = ();

    fn handle(&mut self, msg: UnregisterConnection, _ctx: &mut Self::Context) {
        self.remove_connection(msg.connection);
    }
}

impl Handler<AddKnownNodes> for Server {
    type Result = ();

    fn handle(&mut self, msg: AddKnownNodes, _ctx: &mut Self::Context) {
        for node in msg.0.into_iter() {
            self.add_known_node(node)
        }
    }
}

impl Handler<GetKnownHosts> for Server {
    type Result = Result<Vec<NodeInfo>, ()>;

    fn handle(&mut self, _msg: GetKnownHosts, _ctx: &mut Self::Context) -> Self::Result {
        let known_nodes = self
            .known_nodes
            .iter()
            .map(|(_, node)| node.clone())
            .collect::<Vec<NodeInfo>>();
        Ok(known_nodes)
    }
}

impl Handler<GetReachableHosts> for Server {
    type Result = Result<Vec<NodeId>, ()>;

    fn handle(&mut self, _msg: GetReachableHosts, _ctx: &mut Self::Context) -> Self::Result {
        Ok(self.reachable_nodes.keys()
            .cloned()
            .collect())
    }
}

impl Handler<SendMessage> for Server {
    type Result = ResponseFuture<(), io::Error>;

    fn handle(&mut self, data: SendMessage, ctx: &mut Self::Context) -> Self::Result {
        self.send_message(&data.target, data.msg, ctx)
    }
}

impl Handler<SendRequest> for Server {
    type Result = ResponseActFuture<Server, Message, io::Error>;

    fn handle(&mut self, data: SendRequest, ctx: &mut Self::Context) -> Self::Result {
        self.send_request(&data.target, data.msg, ctx)
    }
}

impl Handler<BroadcastMessage> for Server {
    type Result = ResponseFuture<(), io::Error>;

    fn handle(&mut self, data: BroadcastMessage, ctx: &mut Self::Context) -> Self::Result {
        self.broadcast_message(data.msg, ctx)
    }
}

impl Handler<Subscribe<MessageReceived>> for Server {
    type Result = ();

    fn handle(&mut self, msg: Subscribe<MessageReceived>, _ctx: &mut Self::Context) {
        if self.message_received_sub.is_some() {
            warn!("Subscriber replaced!");
        }
        self.message_received_sub = Some(msg.0)
    }
}

impl Handler<Subscribe<NodeUnreachable>> for Server {
    type Result = ();

    fn handle(&mut self, msg: Subscribe<NodeUnreachable>, _ctx: &mut Self::Context) {
        if self.node_unreachable_sub.is_some() {
            warn!("Subscriber replaced!");
        }
        self.node_unreachable_sub = Some(msg.0)
    }
}

impl Handler<Subscribe<NodeReachable>> for Server {
    type Result = ();

    fn handle(&mut self, msg: Subscribe<NodeReachable>, _ctx: &mut Self::Context) {
        if self.node_reachable_sub.is_some() {
            warn!("Subscriber replaced!");
        }
        self.node_reachable_sub = Some(msg.0)
    }
}

impl Handler<Unsubscribe<MessageReceived>> for Server {
    type Result = ();

    fn handle(&mut self, _: Unsubscribe<MessageReceived>, _ctx: &mut Self::Context) {
        self.message_received_sub = None;
    }
}

impl Handler<NotifyMsgRcvd> for Server {
    type Result = ();

    fn handle(&mut self, msg: NotifyMsgRcvd, ctx: &mut Self::Context) {
        if self.message_received_sub.is_none() {
            panic!("Message received before subscription occurs")
        }

        if let Some(ref sub) = self.message_received_sub {
            let ack_id = msg.ack;
            let sender = msg.sender;

            let res = sub
                .send(msg.into())
                .map_err(|err| {
                    warn!("Error sending message to actor: {:?}", err);
                }).into_actor(self)
                .and_then(move |resp: Option<Message>, server: &mut Server, ctx| {
                    match (resp, ack_id) {
                        (Some(resp), Some(ack_id)) => {
                            let msg = NetMessage::Ack {
                                ack: ack_id,
                                payload: resp,
                            };

                            // How to report this send error?
                            let send_fut = server
                                .send_net_message(&sender, msg, ctx)
                                .map_err(|err| warn!("Cannot send response: {:?}", err));
                            Arbiter::spawn(send_fut);
                        }
                        (None, Some(_ack_id)) => {
                            warn!("Expected response but MessageResponse::None provided");
                        }
                        (Some(_resp), None) => {
                            warn!("Attempt to send unexpected response.");
                        }
                        (None, None) => {}
                    }
                    actix::fut::ok(())
                });

            ctx.spawn(res);
        } else {
            unreachable!()
        }
    }
}

impl Handler<NotifyAckRcvd> for Server {
    type Result = ();

    fn handle(&mut self, msg: NotifyAckRcvd, _ctx: &mut Self::Context) {
        let ack = self.get_ack(&msg.ack);

        if let Some(sender) = ack {
            if sender.send(msg.msg).is_err() {
                // receiver dropped
                warn!("Unexpected ack received")
            }
        } else {
            warn!("Unexpected ack received")
        }
    }
}

#[derive(Clone)]
pub struct ServerRef(Addr<Server>);

impl ServerRef {
    pub fn start(config: Config) -> io::Result<ServerRef> {
        Server::start(config)
    }

    /// Send message and return sending result.
    /// This method does not wait for remote node response. If returned `Future`
    /// object get dropped, message cancels.
    pub fn send_message(
        &self,
        node_id: &NodeId,
        msg: Message,
    ) -> impl Future<Item = (), Error = io::Error> {
        self.0
            .send(SendMessage {
                msg,
                target: node_id.clone(),
            }).map_err(|_| io::Error::new(io::ErrorKind::Other, "Actix mailbox error"))
            .and_then(|res| futures::future::result(res))
    }

    /// Send message to all known nodes except itself.
    /// This method does not wait for remote nodes response. If returned `Future`
    /// object get dropped, message cancels.
    pub fn broadcast(&self, msg: Message) -> impl Future<Item = (), Error = io::Error> {
        self.0
            .send(BroadcastMessage { msg })
            .map_err(|_| io::Error::new(io::ErrorKind::Other, "Actix mailbox error"))
            .and_then(|res| futures::future::result(res))
    }

    /// Send request to `node_id`. Returned future resolves with
    /// remote node response or error.
    pub fn request(
        &self,
        node_id: &NodeId,
        msg: Message,
    ) -> impl Future<Item = Message, Error = io::Error> {
        self.0
            .send(SendRequest {
                msg,
                target: node_id.clone(),
            }).map_err(|_| io::Error::new(io::ErrorKind::Other, "Actix mailbox error"))
            .and_then(|res| futures::future::result(res))
    }

    pub fn subscribe_msg_recvd(&self, recipient: Recipient<MessageReceived>) {
        self.0.do_send(Subscribe(recipient))
    }

    pub fn unsubscribe_msg_recvd(&self, recipient: Recipient<MessageReceived>) {
        self.0.do_send(Unsubscribe(recipient))
    }

    pub fn subscribe_node_reachable(&self, recipient: Recipient<NodeReachable>) {
        self.0.do_send(Subscribe(recipient))
    }

    pub fn subscribe_node_unreachable(&self, recipient: Recipient<NodeUnreachable>) {
        self.0.do_send(Subscribe(recipient))
    }

    pub fn get_reachable_nodes(&self) -> impl Future<Item = Vec<NodeId>, Error = io::Error> {
        self.0.send(GetReachableHosts)
            .map_err(|_| io::Error::new(io::ErrorKind::Other, "Actix mailbox error"))
            .map(|res| res.unwrap())

    }

    pub fn stop(&self) {
        self.0.do_send(Close)
    }

    fn new(addr: Addr<Server>) -> ServerRef {
        ServerRef(addr)
    }

    pub(crate) fn get_known_nodes(&self) -> ActixRequest<Server, GetKnownHosts> {
        self.0.send(GetKnownHosts)
    }

    pub(crate) fn add_known_nodes(&self, nodes: Vec<NodeInfo>) {
        self.0.do_send(AddKnownNodes(nodes))
    }

    // FIXME remove later https://github.com/actix/actix/issues/132
    #[allow(dead_code)]
    pub(crate) fn register_connection(
        &self,
        connection: ConnectionRef,
    ) -> ActixRequest<Server, RegisterConnection> {
        self.0.send(RegisterConnection(connection))
    }

    pub(crate) fn unregister_connection(&self, connection: ConnectionRef) {
        self.0.do_send(UnregisterConnection { connection })
    }

    pub(crate) fn notify_message_received(&self, msg: Message, sender: &NodeId, ack: Option<u64>) {
        let msg = NotifyMsgRcvd {
            msg,
            sender: sender.clone(),
            ack,
        };
        self.0.do_send(msg);
    }

    pub(crate) fn notify_ack_received(&self, msg: Message, ack: u64) {
        let msg = NotifyAckRcvd { msg, ack };
        self.0.do_send(msg);
    }
}
